use crate::{
    auth,
    dbdata::{self, PlaylistConfig},
};

pub fn process_args() -> CliResult {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(|a| a.as_ref()).collect();
    if args.is_empty() {
        return CliResult::Continue(None);
    }

    if let Some((&"user", args)) = args.split_first() {
        let Some((user, args)) = args.split_first() else {
            return ret_error("missing <user>");
        };
        if let Some((&"add", args)) = args.split_first() {
            let Some((password, _)) = args.split_first() else {
                return ret_error("missing <password>");
            };

            let hashed_pw = auth::hash_password(password);
            dbdata::DB.add_user(user, &hashed_pw);
            println!("user {} added", user);
        } else if let Some((&"remove", _)) = args.split_first() {
            let delete_count = dbdata::DB.delete_user(user);

            if delete_count == 0 {
                println!("Did not found any matching user for {}", user);
            } else {
                println!("Successfully deleted user {}", user);
            }
        }
    } else if let Some((&"run", args)) = args.split_first() {
        let Some((config_path, _)) = args.split_first() else {
            return ret_error("missing <config_path>");
        };

        return CliResult::Continue(Some(config_path.to_string()));
    } else if let Some((&"lists", args)) = args.split_first() {
        if let Some((&"add", args)) = args.split_first() {
            let Some((playlist_id, _)) = args.split_first() else {
                return ret_error("missing <list_id>");
            };

            let mut list_conf = PlaylistConfig::new(playlist_id.to_string().into());
            if let Some((jellyfin_playlist, _)) = args.split_first() {
                list_conf.jelly_playlist_id = Some((*jellyfin_playlist).into());
            }

            dbdata::DB.add_playlist_config(&list_conf);
        } else if let Some((&"remove", args)) = args.split_first() {
            let Some((playlist_id, _)) = args.split_first() else {
                return ret_error("missing <list_id>");
            };
            dbdata::DB.delete_playlist_config(&(*playlist_id).into());
        } else if let Some((&"list", _)) = args.split_first() {
            let lists = dbdata::DB.get_playlist_config();
            for list in lists {
                println!(
                    "{} [{}] Jelly:{}",
                    list.playlist_id,
                    if list.enabled { "✅️" } else { "❌️" },
                    list.jelly_playlist_id
                        .as_ref()
                        .map(|j| j.as_ref())
                        .unwrap_or("❌️") 
                );
            }
        }
    } else {
        println!("Invalid cli param {:?}", args);
    }
    return CliResult::Exit;
}

fn ret_error(log: &str) -> CliResult {
    println!("{}", log);
    CliResult::Exit
}

pub enum CliResult {
    Exit,
    Continue(Option<String>), // Config path
}
