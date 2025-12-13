use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error,
    http::{header::{HeaderValue, HeaderName}, Method},
    HttpResponse,
    Result,
};
use futures_util::future::{ok, LocalBoxFuture, Ready};
use std::rc::Rc;

pub struct CorsMiddleware;

impl<S, B> Transform<S, ServiceRequest> for CorsMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = CorsMiddlewareService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(CorsMiddlewareService {
            service: Rc::new(service),
        })
    }
}

pub struct CorsMiddlewareService<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for CorsMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = self.service.clone();
        
        Box::pin(async move {
            let mut res = service.call(req).await?;

            let headers = res.headers_mut();
            headers.insert(HeaderName::from_static("access-control-allow-origin"), HeaderValue::from_static("*"));
            headers.insert(HeaderName::from_static("access-control-allow-methods"), HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS"));
            headers.insert(HeaderName::from_static("access-control-allow-headers"), HeaderValue::from_static("Content-Type, Authorization"));
            headers.insert(HeaderName::from_static("access-control-max-age"), HeaderValue::from_static("86400"));

            Ok(res)
        })
    }
}

pub async fn handle_options() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .insert_header(("access-control-allow-origin", "*"))
        .insert_header(("access-control-allow-methods", "GET, POST, PUT, DELETE, OPTIONS"))
        .insert_header(("access-control-allow-headers", "Content-Type, Authorization"))
        .insert_header(("access-control-max-age", "86400"))
        .finish())
}