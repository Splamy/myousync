use std::path::{Path, PathBuf};

use crate::{brainz::BrainzMetadata, util, MSState};
use anyhow::Context;
use id3::TagLike;
use log::info;
use multitag::{self, data::Album};
use sanitise_file_name::sanitise_with_options;
use walkdir::WalkDir;

pub fn apply_metadata_to_file(path: &Path, tags: &MetadataTags) -> anyhow::Result<()> {
    let mut test_tag = multitag::Tag::read_from_path(path).context("When reading audiotags")?;

    test_tag.set_title(&tags.brainz.title);
    test_tag.set_artist(&tags.brainz.artist.join("; "));
    test_tag.set_album_info(Album {
        title: Some(tags.brainz.album.clone().unwrap_or_default()),
        artist: Some(tags.brainz.artist.join("; ")),
        ..Default::default()
    })?;
    test_tag.set_comment("youtube_id", tags.youtube_id.clone());

    if let Some(brainz_id) = tags.brainz.brainz_recording_id.as_deref() {
        match &mut test_tag {
            multitag::Tag::Id3Tag { inner } => {
                inner.remove_unique_file_identifier_by_owner_identifier("http://musicbrainz.org");
                inner.add_frame(id3::frame::UniqueFileIdentifier {
                    owner_identifier: "http://musicbrainz.org".to_string(),
                    identifier: brainz_id.as_bytes().to_vec(),
                });
            }
            multitag::Tag::OpusTag { .. } => {
                test_tag.set_comment("musicbrainz_trackid", brainz_id.into());
            }
            multitag::Tag::Mp4Tag { .. } => {
                test_tag.set_comment("MusicBrainz Track Id", brainz_id.into());
            }
            multitag::Tag::VorbisFlacTag { .. } => {
                test_tag.set_comment("MUSICBRAINZ_TRACKID", brainz_id.into());
            }
        }
    }

    test_tag.write_to_path(&path)?;
    Ok(())
}

pub fn find_local_file(s: &MSState, video_id: &str) -> Option<PathBuf> {
    for entry in WalkDir::new(&s.config.music)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_dir() {
            continue;
        }
        if let Some(youtube_id) = multitag::Tag::read_from_path(entry.path())
            .ok()
            .and_then(|t| t.get_comment("youtube_id"))
        {
            if youtube_id == video_id {
                info!(
                    "Found already downloaded file by youtube_id: {}",
                    entry.path().display()
                );
                return Some(entry.path().to_path_buf());
            }
        }
    }
    None
}

pub fn move_file_to_library(s: &MSState, path: &Path, tags: &MetadataTags) -> anyhow::Result<()> {
    let clean_title = sanitize_default(&tags.brainz.title);
    let clean_artist = sanitize_default(&tags.brainz.artist.join("; "));
    let clean_album = &tags
        .brainz
        .album
        .clone()
        .map(|a| sanitize_default(&a))
        .unwrap_or_else(|| clean_title.clone());

    let orig_extenstion = path.extension().and_then(|e| e.to_str()).unwrap_or("mp3");

    let mut new_path = s.config.music.clone();
    new_path.push(clean_artist);
    new_path.push(clean_album);

    std::fs::create_dir_all(&new_path)
        .map_err(|e| anyhow::anyhow!("Error creating directory: {}", e))?;

    new_path.push(format!("{}.{}", &clean_title, &orig_extenstion));

    match std::fs::rename(path, &new_path) {
        Ok(_) => {
            cleanup_directory(s, path);
        }
        Err(err_ren) => match std::fs::copy(path, &new_path) {
            Ok(_) => {
                delete_file(s, path)
                    .map_err(|e| anyhow::anyhow!("Error delete after copy file: {}", e))?;
            }
            Err(_) => return Err(anyhow::anyhow!("Error moving file: {}", err_ren)),
        },
    }

    Ok(())
}

pub fn delete_file(s: &MSState, path: &Path) -> anyhow::Result<()> {
    if !path.starts_with(&s.config.music) && !path.starts_with(&s.config.temp) {
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

fn cleanup_directory(s: &MSState, file: &Path) {
    if !file.starts_with(&s.config.music) && !file.starts_with(&s.config.temp) {
        // not in music or temp directory
        return;
    }

    let mut parent = file.parent();
    while let Some(p) = parent {
        // don't delete top level music or temp directory
        if s.config.music.starts_with(p) || s.config.temp.starts_with(p) {
            break;
        }
        if let Ok(cnt) = p.read_dir().map(|r| r.count()) {
            if cnt > 0 {
                break;
            }
            if !std::fs::remove_dir(p).is_ok() {
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
