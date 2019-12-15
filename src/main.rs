use actix_web::{client::Client, web, App, Error, HttpResponse, HttpServer};
use futures::{Future, Stream};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 1. browser requests this service
/// 2. this service handles {LISTEN_HOST}/open/{card_id}
/// 3. sends request to {BACKEND_URL}/cards/{card_id}/meta/
/// 4. converts meta to html meta tags
/// 5. add meta tags to html before </head>
/// 6. sends html to user

fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    pretty_env_logger::init();

    let listen_host = std::env::var("LISTEN_HOST").expect("please, provide LISTEN_HOST");

    let config = Arc::new(Config {
        public_url: std::env::var("PUBLIC_URL").expect("please, provide PUBLIC_URL"),
        image_url: std::env::var("IMAGE_URL").expect("please, provide IMAGE_URL"),
        backend_url: std::env::var("BACKEND_URL").expect("please, provide BACKEND_URL"),
        sitename: std::env::var("SITENAME").expect("please, provide SITENAME"),
        index_html_path: std::env::var("INDEX_HTML_PATH").expect("please, provide INDEX_HTML_PATH"),
    });

    let storage = Arc::new(
        Storage::read_from(config.clone().index_html_path.clone())
            .expect("cannot read INDEX_HTML_FILE"),
    );

    HttpServer::new(move || {
        App::new()
            .data(Client::default())
            .data(config.clone())
            .data(storage.clone())
            .service(web::resource("/open/{card_id}").to_async(card))
            .service(web::resource("/open/{card_id}/").to_async(card))
    })
    .bind(listen_host)?
    .run()
}

#[derive(Debug)]
struct Storage {
    index_html: String,
}

impl Storage {
    pub fn read_from(path: String) -> Result<Self, std::io::Error> {
        let source = std::fs::read_to_string(path)?;

        Ok(Storage { index_html: source })
    }
}

fn create_meta<P, C>(prop: P, content: C) -> String
where
    P: AsRef<str>,
    C: AsRef<str>,
{
    format!(
        r#"<meta property="{}" content="{}" />"#,
        htmlescape::encode_minimal(prop.as_ref()),
        htmlescape::encode_minimal(content.as_ref())
    )
}

#[derive(Debug)]
struct Config {
    public_url: String,
    image_url: String,
    backend_url: String,
    sitename: String,
    index_html_path: String,
}

impl Config {
    fn meta_for_card(&self, card: &Card) -> String {
        let public_url = self.public_url.to_string();

        let title = create_meta("title", &card.title);
        let description = create_meta("description", &card.description);

        let og_sitename = create_meta("og:site_name", &self.sitename);
        let og_type = create_meta("og:type", "article");
        let og_title = create_meta("og:title", &card.title);
        let og_description = create_meta("og:description", &card.description);
        let og_url = create_meta("og:url", format!("{}/open/{}", public_url, card.id));
        let og_image = (card.preview_url.clone()).map_or("".to_string(), |url| {
            create_meta("og_image", format!("{}/{}", self.image_url, url))
        });
        // let og_locale = create_meta("og:locale", "en_US");
        // let og_article_author = create_meta("article:author", "Sergey Sova");
        // let og_article_tag = create_meta("article:tag", "react");
        // https://developer.twitter.com/en/docs/tweets/optimize-with-cards/overview/summary-card-with-large-image
        let og_article_published = create_meta("article:published_time", &card.created_at);
        let og_article_modified = create_meta("article:modified_time", &card.updated_at);

        let twitter_card = create_meta(
            "twitter:card",
            (card.preview_url.clone()).map_or("summary", |_| "summary_large_image"),
        );
        let twitter_site = create_meta("twitter:site", "@howtocards_io");
        let twitter_title = create_meta("twitter:title", &card.title);
        let twitter_description = create_meta("twitter:description", &card.description);
        let twitter_image = (card.preview_url.clone()).map_or("".to_string(), |url| {
            create_meta("twitter:image", format!("{}/{}", self.image_url, url))
        });

        vec![
            title,
            description,
            og_sitename,
            og_type,
            og_title,
            og_description,
            og_url,
            og_image,
            og_article_published,
            og_article_modified,
            twitter_card,
            twitter_site,
            twitter_title,
            twitter_description,
            twitter_image,
        ]
        .iter()
        .fold(String::new(), |acc, meta| format!("{}\n{}", acc, meta))
    }

    fn backend_card_url(&self, card_id: u32) -> String {
        format!("{}/api/cards/{}/meta/", self.backend_url, card_id)
    }
}

#[derive(Debug, Deserialize)]
struct CardPath {
    card_id: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum Answer<T> {
    Err { ok: bool, error: String },
    Ok { ok: bool, result: T },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Card {
    pub title: String,
    pub description: String,
    pub id: i32,
    pub created_at: String,
    pub updated_at: String,
    pub preview_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CardWrapper {
    meta: Card,
}

fn card(
    path: web::Path<CardPath>,
    client: web::Data<Client>,
    config: web::Data<Arc<Config>>,
    storage: web::Data<Arc<Storage>>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let storage_copy = storage.clone();
    client
        .get(config.backend_card_url(path.card_id))
        .send()
        .map_err(Error::from)
        .and_then(|resp| {
            resp.from_err()
                .fold(web::BytesMut::new(), |mut acc, chunk| {
                    acc.extend_from_slice(&chunk);
                    Ok::<_, Error>(acc)
                })
        })
        .and_then(|body| {
            let body: Result<Answer<CardWrapper>, _> = serde_json::from_slice(&body);

            match body {
                Ok(Answer::Ok { result, .. }) => Ok(Some(result.meta)),
                _ => Ok(None),
            }
        })
        .map(move |card| {
            if let Some(card) = card {
                config.meta_for_card(&card)
            } else {
                "<div></div>".to_string()
            }
        })
        .map(move |html| {
            let index_html = storage.index_html.clone();
            let replace_to = format!("{}</head>", html);
            let body = index_html.replace("</head>", &replace_to);

            HttpResponse::build(actix_web::http::StatusCode::OK)
                .content_type("text/html; charset=utf-8")
                .body(&body)
        })
        .or_else(move |err| {
            use log::error;
            let index_html = storage_copy.clone().index_html.clone();

            error!("Failed to get info about card: {:#?}", err);

            HttpResponse::build(actix_web::http::StatusCode::OK)
                .content_type("text/html; charset=utf-8")
                .body(index_html)
        })
}
