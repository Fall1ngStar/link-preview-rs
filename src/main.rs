use actix_cors::Cors;
use actix_web::{get, web, App, HttpRequest, HttpServer, Responder};
use clap::Parser;
use reqwest_middleware::ClientBuilder;
use reqwest_tracing::TracingMiddleware;
use serde::{Deserialize, Serialize};
use tl::{Node, VDom};
use tracing::info;
use tracing_actix_web::TracingLogger;
use tracing_subscriber::EnvFilter;
use url::Url;

static APP_USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux i686; rv:112.0) Gecko/20100101 Firefox/112.0";

#[derive(Deserialize, Debug, Clone)]
struct Params {
    url: Url,
}

fn attr_from_first_query_match(dom: &VDom, query: &str, attr: &str) -> Option<String> {
    let query = dom.query_selector(query);
    let node = query?.next()?.get(dom.parser())?;
    if let Node::Tag(tag) = node {
        let content = tag.attributes().get(attr)?;
        let title = String::from(content?.as_utf8_str());
        return Some(title);
    }
    None
}

fn get_absolute_path(url: &Url, relative_path: String) -> Option<Url> {
    url.join(&relative_path).ok()
}

fn title_from_title_tag(dom: &VDom) -> Option<String> {
    let query = dom.query_selector("title");
    let node = query?.next()?.get(dom.parser())?;
    if let Node::Tag(tag) = node {
        let title = String::from(tag.inner_text(dom.parser()));
        return Some(title);
    }
    None
}

fn get_title(dom: &VDom) -> Option<String> {
    if let Some(title) = attr_from_first_query_match(dom, "meta[property='og:title']", "content") {
        return Some(title);
    }
    title_from_title_tag(dom)
}

fn get_description(dom: &VDom) -> Option<String> {
    attr_from_first_query_match(&dom, "meta[property='og:description']", "content")
}

fn get_domain(url: &Url) -> Option<String> {
    let host = String::from(url.host_str()?);
    Some(host.replace("www.", ""))
}

fn get_favicon(dom: &VDom, url: &Url) -> Option<Url> {
    let favicon = attr_from_first_query_match(dom, "link[rel='icon']", "href")?;
    get_absolute_path(url, favicon)
}

fn get_image(dom: &VDom, url: &Url) -> Option<Url> {
    let image = attr_from_first_query_match(dom, "meta[property='og:image']", "content")?;
    get_absolute_path(url, image)
}

fn get_og_url(dom: &VDom) -> Option<String> {
    attr_from_first_query_match(&dom, "meta[property='og:url']", "content")
}

fn get_sitename(dom: &VDom) -> Option<String> {
    attr_from_first_query_match(&dom, "meta[property='og:site_name']", "content")
}

fn get_type(dom: &VDom) -> Option<String> {
    attr_from_first_query_match(&dom, "meta[property='og:type']", "content")
}

#[derive(Serialize, Debug)]
struct Response {
    title: Option<String>,
    description: Option<String>,
    domain: Option<String>,
    favicon: Option<Url>,
    image: Option<Url>,
    og_url: Option<String>,
    sitename: Option<String>,
    #[serde(rename = "type")]
    site_type: Option<String>,
}

#[get("/")]
async fn root(params: web::Query<Params>, request: HttpRequest) -> impl Responder {
    let user_agent = request
        .headers()
        .get("User-Agent")
        .map_or(APP_USER_AGENT, |val| val.to_str().unwrap());

    let reqwest_client = reqwest::Client::builder()
        .user_agent(user_agent)
        .build()
        .unwrap();

    let client = ClientBuilder::new(reqwest_client)
        .with(TracingMiddleware::default())
        .build();

    let response = client.get(params.url.clone()).send().await.unwrap();
    let content = response.text().await.unwrap();
    let dom = tl::parse(&content, tl::ParserOptions::default()).unwrap();
    let title = get_title(&dom);
    let description = get_description(&dom);
    let domain = get_domain(&params.url);
    let favicon = get_favicon(&dom, &params.url);
    let image = get_image(&dom, &params.url);
    let og_url = get_og_url(&dom);
    let sitename = get_sitename(&dom);
    let site_type = get_type(&dom);
    web::Json(Response {
        title,
        description,
        domain,
        favicon,
        image,
        og_url,
        sitename,
        site_type,
    })
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value_t = String::from("localhost"))]
    hostname: String,
    #[arg(long, default_value_t = 3001)]
    port: u16,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();
    let args = Args::parse();
    info!("Args: {:?}", args);
    HttpServer::new(|| {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header();

        App::new()
            .wrap(cors)
            .wrap(TracingLogger::default())
            .service(root)
    })
    .bind((args.hostname, args.port))?
    .run()
    .await
}
