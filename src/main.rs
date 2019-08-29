use actix_web::{client::Client, web, App, Error, HttpResponse, HttpServer};
use futures::{Future, Stream};
use serde::{Deserialize, Serialize};

/// 1. browser requests this service
/// 2. this service handles /open/{card_id}
/// 3. sends request to backend /cards/{card_id}/meta/
/// 4. converts json meta tags to html meta tags
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

trait MetaTags {
    fn to_meta(&self) -> String;
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

impl MetaTags for Card {
    fn to_meta(&self) -> String {
        let frontend_base_url = "https://test.cards.atomix.team".to_string();

        let og_type = create_meta("og:type", "article");
        let og_title = create_meta("og:title", &self.title);
        let og_url = create_meta("og:url", format!("{}/open/{}", frontend_base_url, self.id));
        let og_image =
            (self.preview.clone()).map_or("".to_string(), |url| create_meta("og_image", url));
        let og_published = create_meta("article:published_time", &self.created_at);
        let og_modified = create_meta("article:modified_time", &self.updated_at);

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
}

#[derive(Debug, Deserialize, Serialize)]
struct CardWrapper {
    meta: Card,
}

fn card(
    path: web::Path<CardPath>,
    client: web::Data<Client>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    client
        .get(format!(
            "http://localhost:9000/api/cards/{}/meta/",
            path.card_id
        ))
        .send()
        .map_err(Error::from)
        .and_then(|resp| {
            println!("{:?}", resp);
            resp.from_err()
                .fold(web::BytesMut::new(), |mut acc, chunk| {
                    acc.extend_from_slice(&chunk);
                    Ok::<_, Error>(acc)
                })
                .and_then(|body| {
                    let body: Result<Answer<CardWrapper>, _> = serde_json::from_slice(&body);

                    println!("{:#?}", body);

                    match body {
                        Ok(Answer::Ok { result, .. }) => Ok(Some(result.meta)),
                        _ => Ok(None),
                    }
                })
                .map(|card| {
                    if let Some(card) = card {
                        card.to_meta()
                    } else {
                        "<div></div>".to_string()
                    }
                })
                .map(|html| {
                    HttpResponse::build(actix_web::http::StatusCode::OK)
                        .content_type("text/html; charset=utf-8")
                        .body(&html)
                })
        })
}

fn main() -> std::io::Result<()> {
    pretty_env_logger::init();

    HttpServer::new(|| {
        App::new()
            .data(Client::default())
            .service(web::resource("/open/{card_id}").to_async(card))
            .service(web::resource("/open/{card_id}/").to_async(card))
    })
    .bind("127.0.0.1:3000")?
    .run()
}
