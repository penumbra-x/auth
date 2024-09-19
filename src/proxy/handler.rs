use super::mitm::RequestOrResponse;
use hyper::{Body, Request, Response};

pub trait HttpHandler: Clone + Send + Sync + 'static {
    fn handle_request(&self, req: Request<Body>) -> RequestOrResponse {
        RequestOrResponse::Request(req)
    }

    fn handle_response(&self, res: Response<Body>) -> Response<Body> {
        res
    }
}
