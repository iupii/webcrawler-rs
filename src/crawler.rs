use crossbeam_deque::Steal::Success;
use crossbeam_deque::Worker;
use crossbeam_utils::thread::scope;
use scraper::{Html, Selector};
use url::Url;

use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::mpsc::channel;
use std::sync::Arc;

use crate::http_client::HTTPClient;

pub struct Crawler {
    base: Url,
    limit: usize,
}

impl Crawler {
    pub fn new(base: Url, limit: usize) -> Self {
        Crawler { base, limit }
    }

    pub fn run<T: HTTPClient>(&self, http_client: &T) {
        let (links, fails) = self.crawl(http_client);
        println!("links {:?}:", links.len());
        for link in links {
            println!("\t{:?}", link);
        }
        if !fails.is_empty() {
            println!("\nwith fails {:?}:", fails.len());
            for (link, err) in fails {
                println!("\t{:?}: {:?}", link, err);
            }
        }
    }

    fn init(&self, links: &mut HashSet<String>, worker: &Worker<String>) -> i32 {
        let url = String::from(self.base.as_str());
        worker.push(url.clone());
        links.insert(url);
        let mut remaining = 1;
        if self.base.path() != "/" || self.base.query().is_some() {
            let mut url = self.base.join("/").unwrap();
            url.set_query(None);
            let url = url.into_string();
            worker.push(url.clone());
            links.insert(url);
            remaining += 1;
        }
        remaining
    }

    fn crawl<T: HTTPClient>(&self, http_client: &T) -> (HashSet<String>, Vec<(String, String)>) {
        scope(|scope| {
            let is_done = Arc::new(AtomicBool::new(false));
            let worker: Worker<String> = Worker::new_fifo();
            let (sender, receiver) = channel();

            for _ in 0..self.limit {
                let is_done = is_done.clone();
                let stealer = worker.stealer();
                let sender = sender.clone();

                scope.spawn(move |_| {
                    while !is_done.load(SeqCst) {
                        if let Success(link) = stealer.steal() {
                            match http_client.fetch(&link) {
                                Ok(document) => {
                                    let links = get_internal_links(&self.base, &document);
                                    sender.send(Ok(links)).unwrap();
                                }
                                Err(err) => {
                                    sender.send(Err((link.clone(), err))).unwrap();
                                }
                            }
                        }
                    }
                });
            }

            let mut links = HashSet::new();
            let mut fails = Vec::new();
            let mut remaining = self.init(&mut links, &worker);
            while remaining > 0 {
                match receiver.recv().unwrap() {
                    Ok(new_links) => {
                        remaining -= 1;
                        for link in new_links {
                            if links.insert(link.clone()) {
                                worker.push(link);
                                remaining += 1;
                            }
                        }
                    }
                    Err((link, err)) => {
                        remaining -= 1;
                        fails.push((link, err));
                    }
                }
            }
            is_done.store(true, SeqCst);
            (links, fails)
        })
        .unwrap()
    }
}

fn get_internal_links(base: &Url, html: &str) -> HashSet<String> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("a").unwrap();
    document
        .select(&selector)
        .filter_map(|v| v.value().attr("href"))
        .filter_map(|v| base.join(v).ok())
        .filter(|v| v.domain() == base.domain())
        .map(move |mut v| {
            v.set_fragment(None);
            v.into_string()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_get_internal_links() {
        let base = Url::parse("http://internal.com").unwrap();
        let html = r#"
            <!DOCTYPE html>
            <meta charset="utf-8">
            <title>Hello, world!</title>
            <body>
            <ul>
                <li><a href="http://internal.com/abs#one">abs url</a></li>
                <li><a href="/rel#two">rel url</a></li>
                <li><a href="http://external.com">ext url</a></li>
                <li><a>non url</a></li>
            </ul>
            </body>
        "#;
        let links = get_internal_links(&base, html);
        assert_eq!(
            links,
            [
                String::from("http://internal.com/abs"),
                String::from("http://internal.com/rel")
            ]
            .iter()
            .cloned()
            .collect()
        );
    }

    use std::collections::HashMap;

    struct MockHttpClient {
        documents: HashMap<String, String>,
    }

    impl HTTPClient for MockHttpClient {
        fn fetch(&self, url: &str) -> Result<String, String> {
            match self.documents.get(url) {
                Some(document) => Ok(document.clone()),
                None => Err(String::from("404")),
            }
        }
    }

    #[test]
    fn check_crawler() {
        let mut documents = HashMap::new();
        documents.insert(
            "http://internal.com/".to_string(),
            r#"
                    <!DOCTYPE html>
                    <meta charset="utf-8">
                    <title>Main</title>
                    <body>
                    <ul>
                        <li><a href="http://internal.com/abs#one">abs url</a></li>
                        <li><a href="/rel#two">rel url</a></li>
                        <li><a href="http://external.com">ext url</a></li>
                        <li><a>non url</a></li>
                    </ul>
                    </body>
                "#
            .to_string(),
        );
        documents.insert(
            "http://internal.com/abs".to_string(),
            r#"
                    <!DOCTYPE html>
                    <meta charset="utf-8">
                    <title>Abs</title>
                    <body>
                    <ul>
                        <li><a href="http://internal.com/abs_second">abs url</a></li>
                        <li><a href="/rel">rel url</a></li>
                        <li><a href="http://external.com">ext url</a></li>
                        <li><a>non url</a></li>
                    </ul>
                    </body>
                "#
            .to_string(),
        );
        documents.insert(
            "http://internal.com/rel".to_string(),
            r#"
                    <!DOCTYPE html>
                    <meta charset="utf-8">
                    <title>Rel</title>
                    <body>
                    <ul>
                        <li><a href="http://internal.com/abs">abs url</a></li>
                        <li><a href="/rel_second">rel url</a></li>
                        <li><a href="http://external.com">ext url</a></li>
                        <li><a>non url</a></li>
                    </ul>
                    </body>
                "#
            .to_string(),
        );
        documents.insert(
            "http://internal.com/abs_second".to_string(),
            r#"
                    <!DOCTYPE html>
                    <meta charset="utf-8">
                    <title>Abs Second</title>
                    <body>
                    <ul>
                        <li><a href="http://internal.com/abs">abs url</a></li>
                        <li><a href="/rel_second">rel url</a></li>
                        <li><a href="http://external.com">ext url</a></li>
                        <li><a>non url</a></li>
                    </ul>
                    </body>
                "#
            .to_string(),
        );
        documents.insert(
            "http://internal.com/rel_second".to_string(),
            r#"
                    <!DOCTYPE html>
                    <meta charset="utf-8">
                    <title>Rel Second</title>
                    <body>
                    <ul>
                        <li><a href="http://internal.com/abs">abs url</a></li>
                        <li><a href="/rel_second">rel url</a></li>
                        <li><a href="http://external.com">ext url</a></li>
                        <li><a>non url</a></li>
                    </ul>
                    </body>
                "#
            .to_string(),
        );
        let url = Url::parse("http://internal.com").unwrap();
        let (links, fails) = Crawler::new(url, 5).crawl(&MockHttpClient { documents });
        assert!(fails.is_empty());
        assert_eq!(links.len(), 5);
        assert_eq!(
            links,
            [
                String::from("http://internal.com/"),
                String::from("http://internal.com/abs"),
                String::from("http://internal.com/rel"),
                String::from("http://internal.com/abs_second"),
                String::from("http://internal.com/rel_second"),
            ]
            .iter()
            .cloned()
            .collect()
        );
    }
}
