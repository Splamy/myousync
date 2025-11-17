use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{MsPaths, MsState, brainz::BrainzMetadata, dbdata};
use anyhow::Context;
use id3::TagLike;
use log::info;
use multitag::{self, data::Album};
use sanitise_file_name::sanitise_with_options;
use walkdir::WalkDir;

pub fn apply_metadata_to_file(path: &Path, tags: &MetadataTags) -> anyhow::Result<()> {
    let mut tag = multitag::Tag::read_from_path(path).context("When reading audiotags")?;

    tag.remove_title();
    tag.set_title(&tags.brainz.title);
    tag.remove_artist();
    tag.set_artist(&tags.brainz.artist.join("; "));
    let mut album = tag.get_album_info().unwrap_or(Album::default());
    album.title = Some(tags.brainz.album.clone().unwrap_or_default());
    album.artist = Some(tags.brainz.artist.join("; "));
    tag.remove_all_album_info();
    tag.set_album_info(album)?;
    tag.set_comment("youtube_id", tags.youtube_id.clone());

    if let Some(brainz_id) = tags.brainz.brainz_recording_id.as_deref() {
        match &mut tag {
            multitag::Tag::Id3Tag { inner } => {
                inner.remove_unique_file_identifier_by_owner_identifier("http://musicbrainz.org");
                inner.add_frame(id3::frame::UniqueFileIdentifier {
                    owner_identifier: "http://musicbrainz.org".to_string(),
                    identifier: brainz_id.as_bytes().to_vec(),
                });
            }
            multitag::Tag::OpusTag { .. } => {
                tag.set_comment("musicbrainz_trackid", brainz_id.into());
            }
            multitag::Tag::Mp4Tag { .. } => {
                tag.set_comment("MusicBrainz Track Id", brainz_id.into());
            }
            multitag::Tag::VorbisFlacTag { .. } => {
                tag.set_comment("MUSICBRAINZ_TRACKID", brainz_id.into());
            }
            multitag::Tag::OggTag { .. } => {
                unimplemented!()
            }
        }
    }

    tag.write_to_path(path)?;
    Ok(())
}

pub fn find_local_file(s: &MsState, video_id: &str) -> Option<PathBuf> {
    let mut cache = s.file_cache.lock().unwrap();
    if let Some(path) = cache.get(video_id) {
        if check_file(path, video_id) {
            return Some(path.clone());
        }
    }

    if dbdata::DB.get_video_fetch_status(video_id) == Some(dbdata::FetchStatus::Disabled) {
        return None;
    }

    cache.clear();
    info!("Rebuilding file cache");
    create_cache(&s.config.paths.music, &mut cache);
    if let Some(migrate) = &s.config.paths.migrate {
        info!("Rebuilding migrate cache");
        create_cache(migrate, &mut cache);
    }
    info!("Cache rebuilt with {} entries", cache.len());

    if let Some(path) = cache.get(video_id) {
        return Some(path.clone());
    }

    None
}

fn create_cache(path: &Path, map: &mut HashMap<String, PathBuf>) {
    map.extend(
        WalkDir::new(path)
            .into_iter()
            .filter_map(|p| p.ok())
            .filter(|p| p.file_type().is_file())
            .map(|f| f.into_path())
            .flat_map(|p| multitag::Tag::read_from_path(&p).ok().map(|t| (t, p)))
            .flat_map(|(t, p)| t.get_comment("youtube_id").map(|y| (y, p))),
    );
}

fn check_file(path: &Path, video_id: &str) -> bool {
    multitag::Tag::read_from_path(path)
        .ok()
        .and_then(|t| t.get_comment("youtube_id"))
        .map(|y| y == video_id)
        .unwrap_or(false)
}

pub fn move_file_to_library(s: &MsState, path: &Path, tags: &MetadataTags) -> anyhow::Result<()> {
    let clean_title = sanitize_default(&tags.brainz.title);
    let clean_artist = sanitize_default(&tags.brainz.artist.join("; "));
    let clean_album = &tags
        .brainz
        .album
        .clone()
        .map(|a| sanitize_default(&a))
        .unwrap_or_else(|| clean_title.clone());

    let orig_extenstion = path.extension().and_then(|e| e.to_str()).unwrap_or("mp3");

    let mut new_path = s.config.paths.music.clone();
    new_path.push(clean_artist);
    new_path.push(clean_album);

    std::fs::create_dir_all(&new_path)
        .map_err(|e| anyhow::anyhow!("Error creating directory: {}", e))?;

    new_path.push(format!("{}.{}", &clean_title, &orig_extenstion));

    move_file(&s.config.paths, path, &new_path)?;

    let mut cache = s.file_cache.lock().unwrap();
    cache.remove(&tags.youtube_id);
    cache.insert(tags.youtube_id.clone(), new_path);

    Ok(())
}

pub fn delete_file(s: &MsPaths, path: &Path) -> anyhow::Result<()> {
    if !s.is_sub_file(path) {
        // not in music or temp directory
        return Err(anyhow::anyhow!("Not in music or temp directory"));
    }
    match std::fs::remove_file(path) {
        Ok(_) => {
            cleanup_directory(s, path);
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!("Error deleting file: {}", e)),
    }
}

fn move_file(s: &MsPaths, path: &Path, new_path: &Path) -> anyhow::Result<()> {
    match std::fs::rename(path, new_path) {
        Ok(_) => {
            cleanup_directory(s, path);
            Ok(())
        }
        Err(err_ren) => match std::fs::copy(path, new_path) {
            Ok(_) => delete_file(s, path)
                .map_err(|e| anyhow::anyhow!("Error delete after copy file: {}", e)),
            Err(_) => Err(anyhow::anyhow!("Error moving file: {}", err_ren)),
        },
    }
}

fn cleanup_directory(s: &MsPaths, file: &Path) {
    if !s.is_sub_file(file) {
        return;
    }

    let mut parent = file.parent();
    while let Some(p) = parent {
        // don't delete top level music or temp directory
        if !s.is_sub_file(p) {
            break;
        }
        if let Ok(cnt) = p.read_dir().map(|r| r.count()) {
            if cnt > 0 {
                break;
            }
            if std::fs::remove_dir(p).is_err() {
                break;
            }
            parent = p.parent();
        } else {
            break;
        }
    }
}

static SANITIZE_OPTIONS: sanitise_file_name::Options<Option<char>> = sanitise_file_name::Options {
    length_limit: 64,
    extension_cleverness: false,
    most_fs_safe: true,
    windows_safe: true,
    url_safe: true,
    normalise_whitespace: true,
    trim_spaces_and_full_stops: true,
    trim_more_punctuation: true,
    six_measures_of_barley: "song",
    ..sanitise_file_name::Options::DEFAULT
};

fn sanitize_default(s: &str) -> String {
    sanitise_with_options(s, &SANITIZE_OPTIONS)
}

pub struct MetadataTags {
    pub youtube_id: String,
    pub brainz: BrainzMetadata,
}
