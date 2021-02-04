use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::RwLock;
use useful_static::RawGlobal;

#[derive(serde::Deserialize)]
struct TSettings {
    root: String,
    ip: Option<String>,
    port: Option<String>,
    notfound: Option<String>,
    badreq: Option<String>,
}

struct Settings {
    root: std::path::PathBuf,
    ipport: SocketAddr,
    notfound: Vec<u8>,
    badreq: Vec<u8>,
}

static PROFILE: RawGlobal<RwLock<Settings>> = RawGlobal::new();

#[tokio::main]
async fn main() {
    let mut settings = String::new();
    match File::open("./settings.txt").await {
        Ok(mut o) => o.read_to_string(&mut settings).await.unwrap(),
        Err(_) => panic!("Auto config generating is stub."),
    };
    let settings: TSettings = toml::from_str(&settings).unwrap();
    PROFILE.set(RwLock::new(Settings {
        root: std::env::current_dir()
            .unwrap()
            .join(&settings.root)
            .canonicalize()
            .unwrap(),
        ipport: ipport_parse(&settings.ip, &settings.port)
            .await
            .parse()
            .unwrap(),
        notfound: dir_parse(&settings.notfound, "Not found.").await,
        badreq: dir_parse(&settings.badreq, "Bad request.").await,
    }));
    drop(settings);
    let make_service = make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle)) });
    let server = Server::bind(&PROFILE.read().await.ipport).serve(make_service);
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

async fn handle(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    if hyper::Method::GET == req.method() {
        return match get(req).await {
            Ok(o) => Ok(o),
            Err(_) => Ok(badreq().await),
        };
    } else if hyper::Method::HEAD == req.method() {
        return match head(req).await {
            Ok(o) => Ok(o),
            Err(_) => Ok(badreq().await),
        };
    } else {
        Ok(badreq().await)
    }
}

async fn get(req: Request<Body>) -> Result<Response<Body>, std::io::Error> {
    let mut buffer = Vec::new();
    let mut path = match getpath(&req).await {
        Ok(o) => o,
        Err(_) => return Ok(notfound().await),
    };
    if path.starts_with(&PROFILE.read().await.root) {
        if path.is_file() {
        } else {
            path = path.join("index.html");
        }
        File::open(path).await?.read_to_end(&mut buffer).await?;
        Ok(Response::new(Body::from(buffer)))
    } else {
        Ok(badreq().await)
    }
}

async fn head(req: Request<Body>) -> Result<Response<Body>, std::io::Error> {
    let mut path = match getpath(&req).await {
        Ok(o) => o,
        Err(_) => return Ok(notfound_size().await),
    };
    if path.starts_with(&PROFILE.read().await.root) {
        if path.is_file() {
        } else {
            path = path.join("index.html");
        }
        Ok(Response::builder()
            .status(200)
            .header("content-length", tokio::fs::metadata(path).await?.len())
            .body(Body::empty())
            .unwrap())
    } else {
        Ok(badreq_size().await)
    }
}

//http status
async fn badreq() -> Response<Body> {
    Response::builder()
        .status(400)
        .body(Body::from(PROFILE.read().await.badreq.clone()))
        .unwrap()
}

async fn notfound() -> Response<Body> {
    Response::builder()
        .status(404)
        .body(Body::from(PROFILE.read().await.notfound.clone()))
        .unwrap()
}

async fn badreq_size() -> Response<Body> {
    Response::builder()
        .status(400)
        .header("content-length", PROFILE.read().await.badreq.len())
        .body(Body::empty())
        .unwrap()
}

async fn notfound_size() -> Response<Body> {
    Response::builder()
        .status(404)
        .header("content-length", PROFILE.read().await.notfound.len())
        .body(Body::empty())
        .unwrap()
}

//parse setting file
async fn ipport_parse(ip: &Option<String>, port: &Option<String>) -> String {
    let ip2 = match ip {
        Some(s) => s,
        None => "0.0.0.0",
    };
    let port2 = match port {
        Some(s) => s,
        None => "80",
    };
    String::from(ip2) + ":" + port2
}

async fn dir_parse(dir: &Option<String>, default: &str) -> Vec<u8> {
    let mut dir2 = Vec::new();
    match dir {
        Some(s) => {
            File::open(s)
                .await
                .unwrap()
                .read_to_end(&mut dir2)
                .await
                .unwrap();
        }
        None => {
            dir2 = String::from(default).into_bytes();
        }
    }
    dir2
}

//other
async fn getpath(req: &Request<Body>) -> std::io::Result<std::path::PathBuf> {
    let mut path = PROFILE.read().await.root.clone();
    for i in req.uri().path().split("/") {
        path.push(i);
    }
    path.canonicalize()
}
