#![warn(rust_2018_idioms)]
use clap::{value_t_or_exit, App, Arg};
use url::Url;

use crate::crawler::*;
use crate::http_client::Reqwest;

mod crawler;
mod http_client;

fn main() {
    let args = App::new("webcrawler")
        .version("0.1.0")
        .about("Web crawler")
        .arg(
            Arg::with_name("url")
                .short("u")
                .help("Url to crawl (http://example.com)")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("limit")
                .short("l")
                .help("Rate limit")
                .takes_value(true)
                .required(true)
                .default_value("5"),
        )
        .get_matches();

    // TODO: unify error messages
    let url = args.value_of("url").unwrap();
    let limit: usize = value_t_or_exit!(args, "limit", usize);

    let url = Url::parse(url).unwrap_or_else(|e| {
        panic!("error: {}", e);
    });

    let _domain = url.domain().unwrap_or_else(|| {
        panic!("error: Domain expected");
    });

    Crawler::new(url, limit).run(&Reqwest);
}
