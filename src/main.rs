use actix_web::{client::Client, web, App, Error, HttpResponse, HttpServer, Responder};
use futures::{Future, Stream};
use serde::{Deserialize, Serialize};

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
    id: u32,
    #[serde(skip)]
    author_id: u32,
    title: String,
    #[serde(skip)]
    content: serde_json::Value,
    created_at: String,
    updated_at: String,
    useful_for: u32,
    #[serde(skip)]
    meta: serde_json::Value,
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
        let og_type = create_meta("og:type", "article");
        let og_title = create_meta("og:title", &self.title);
        let og_published = create_meta("article:published_time", &self.created_at);
        let og_modified = create_meta("article:modified_time", &self.updated_at);

        vec![og_type, og_title, og_published, og_modified]
            .iter()
            .fold(String::new(), |acc, meta| format!("{}\n{}", acc, meta))
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct CardWrapper {
    card: Card,
}

fn card(
    path: web::Path<CardPath>,
    client: web::Data<Client>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    client
        .get(format!("http://localhost:9000/api/cards/{}/", path.card_id))
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
                        Ok(Answer::Ok { result, .. }) => Ok(Some(result.card)),
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
