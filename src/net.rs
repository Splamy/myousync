use std::sync::LazyLock;
use reqwest::Client;

pub static CLIENT: LazyLock<Client> = LazyLock::new(|| Client::new());
