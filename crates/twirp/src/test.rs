//! Test helpers and mini twirp api server implementation.
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Server};
use serde::de::DeserializeOwned;
use tokio::task::JoinHandle;
use url::Url;

use crate::client::{request, HttpTwirpClient, TwirpClientError};
use crate::*;

pub async fn run_test_server(port: u16) -> JoinHandle<Result<(), hyper::Error>> {
    let router = test_api_router().await;
    let service = make_service_fn(move |_| {
        let router = router.clone();
        async { Ok::<_, GenericError>(service_fn(move |req| crate::serve(router.clone(), req))) }
    });

    let addr = ([127, 0, 0, 1], port).into();
    let server = Server::bind(&addr).serve(service);
    println!("Listening on {addr}");
    let h = tokio::spawn(server);
    tokio::time::sleep(Duration::from_millis(100)).await;
    h
}

pub async fn test_api_router() -> Arc<Router> {
    let api = Arc::new(TestAPIServer {});
    let mut router = Router::default();
    // NB: This would be generated
    {
        let api = api.clone();
        router.add_method("test.TestAPI/Ping", move |req| {
            let api = api.clone();
            async move { api.ping(req).await }
        });
    }
    {
        router.add_method("test.TestAPI/Boom", move |req| {
            let api = api.clone();
            async move { api.boom(req).await }
        });
    }
    Arc::new(router)
}

pub fn gen_ping_request(name: &str) -> Request<hyper::Body> {
    let req = serde_json::to_string(&PingRequest {
        name: name.to_string(),
    })
    .expect("will always be valid json");
    Request::post("/twirp/test.TestAPI/Ping")
        .body(Body::from(req))
        .expect("always a valid twirp request")
}

pub async fn read_string_body(body: Body) -> String {
    let data = hyper::body::to_bytes(body)
        .await
        .expect("invalid body")
        .to_vec();
    String::from_utf8(data).expect("non-utf8 body")
}

pub async fn read_json_body<T>(body: Body) -> T
where
    T: DeserializeOwned,
{
    let data = hyper::body::to_bytes(body)
        .await
        .expect("invalid body")
        .to_vec();
    serde_json::from_slice(&data).expect("twirp response isn't valid JSON")
}

pub async fn read_err_body(body: Body) -> TwirpErrorResponse {
    read_json_body(body).await
}

// Hand written sample test server and client

pub struct TestAPIServer;

#[async_trait]
impl TestAPI for TestAPIServer {
    async fn ping(&self, req: PingRequest) -> Result<PingResponse, TwirpErrorResponse> {
        Ok(PingResponse { name: req.name })
    }

    async fn boom(&self, _: PingRequest) -> Result<PingResponse, TwirpErrorResponse> {
        Err(internal("boom!"))
    }
}

// Hand written custom client
// Custom client: add extra headers, do logging, etc
pub struct TestAPIClientCustom {
    pub hmac_key: Option<String>,
    pub client: HttpTwirpClient,
}

impl TestAPIClientCustom {
    pub async fn ping(
        &self,
        hostname: &str,
        req: PingRequest,
    ) -> crate::client::Result<PingResponse> {
        let mut url = self.ping_url(&self.client.base_url)?;
        url.set_host(Some(hostname))?;
        self.ping_inner(url, req).await
    }
}

#[async_trait]
impl TestAPIClientExt for TestAPIClientCustom {
    async fn ping_inner(&self, url: Url, req: PingRequest) -> crate::client::Result<PingResponse> {
        let mut r = self
            .client
            .client
            .post(url)
            .header("X-GitHub-Request-Id", "XYZ");
        if let Some(_hmac_key) = &self.hmac_key {
            r = r.header("Request-HMAC", "example:todo");
        }
        request(r, req).await
    }
}

// Small test twirp services (this would usually be generated with twirp-build)

#[async_trait]
pub trait TestAPIClientExt {
    fn ping_url(&self, base_url: &Url) -> Result<Url, TwirpClientError> {
        let url = base_url.join("test.TestAPI/Ping")?;
        Ok(url)
    }
    async fn ping_inner(
        &self,
        url: Url,
        req: PingRequest,
    ) -> Result<PingResponse, TwirpClientError>;
}

#[async_trait]
impl TestAPIClientExt for HttpTwirpClient {
    async fn ping_inner(
        &self,
        url: Url,
        req: PingRequest,
    ) -> Result<PingResponse, TwirpClientError> {
        request(self.client.post(url), req).await
    }
}

#[async_trait]
pub trait TestAPIClient {
    async fn ping(&self, req: PingRequest) -> Result<PingResponse, TwirpClientError>;
    async fn boom(&self, req: PingRequest) -> Result<PingResponse, TwirpClientError>;
}

#[async_trait]
impl TestAPIClient for HttpTwirpClient {
    async fn ping(&self, req: PingRequest) -> Result<PingResponse, TwirpClientError> {
        self.ping_inner(self.ping_url(&self.base_url)?, req).await
    }

    async fn boom(&self, _req: PingRequest) -> Result<PingResponse, TwirpClientError> {
        todo!()
    }
}

#[async_trait]
pub trait TestAPI {
    async fn ping(&self, req: PingRequest) -> Result<PingResponse, TwirpErrorResponse>;
    async fn boom(&self, req: PingRequest) -> Result<PingResponse, TwirpErrorResponse>;
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PingRequest {
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PingResponse {
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
}
