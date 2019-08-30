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
    pub preview: Option<String>,
}

fn create_meta<P, C>(prop: P, content: C) -> String
where
    P: AsRef<str>,
    C: AsRef<str>,
{
    format!(
        r#"<meta property="{}" content="{}" />"#,
        prop.as_ref(),
        content.as_ref()
    )
}

#[derive(Debug)]
struct Config {
    public_url: String,
    backend_url: String,
}

impl Config {
    fn meta_for_card(&self, card: &Card) -> String {
        let public_url = self.public_url.to_string();

        let og_type = create_meta("og:type", "article");
        let og_title = create_meta("og:title", &card.title);
        let og_url = create_meta("og:url", format!("{}/open/{}", public_url, card.id));
        let og_image =
            (card.preview.clone()).map_or("".to_string(), |url| create_meta("og_image", url));
        let og_published = create_meta("article:published_time", &card.created_at);
        let og_modified = create_meta("article:modified_time", &card.updated_at);

        vec![
            og_type,
            og_title,
            og_url,
            og_image,
            og_published,
            og_modified,
        ]
        .iter()
        .fold(String::new(), |acc, meta| format!("{}\n{}", acc, meta))
    }

    fn backend_card_url(&self, card_id: u32) -> String {
        format!("{}/api/cards/{}/meta/", self.backend_url, card_id)
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct CardWrapper {
    meta: Card,
}

fn card(
    path: web::Path<CardPath>,
    client: web::Data<Client>,
    config: web::Data<Arc<Config>>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let config = config;
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
        .map(|html| {
            HttpResponse::build(actix_web::http::StatusCode::OK)
                .content_type("text/html; charset=utf-8")
                .body(&html)
        })
}

fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    pretty_env_logger::init();

    let listen_host = std::env::var("LISTEN_HOST").expect("please, provide LISTEN_HOST");

    let public_url = std::env::var("PUBLIC_URL").expect("please, provide PUBLIC_URL");
    let backend_url = std::env::var("BACKEND_URL").expect("please, provide BACKEND_URL");
    let config = Arc::new(Config {
        public_url,
        backend_url,
    });

    HttpServer::new(move || {
        App::new()
            .data(Client::default())
            .data(config.clone())
            .service(web::resource("/open/{card_id}").to_async(card))
            .service(web::resource("/open/{card_id}/").to_async(card))
    })
    .bind(listen_host)?
    .run()
}
