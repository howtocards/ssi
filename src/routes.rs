

#[derive(Debug, Deserialize, Serialize)]
struct CardWrapper {
    meta: Card,
}

pub fn card(
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
