use super::mitm::RequestOrResponse;
use hyper::{Body, Request, Response};

pub trait HttpHandler: Clone + Send + Sync + 'static {
    fn handle_request(
        &self,
        req: Request<Body>,
    ) -> impl std::future::Future<Output = RequestOrResponse> + Send {
        async { RequestOrResponse::Request(req) }
    }

    fn handle_response(&self, res: Response<Body>) -> Response<Body> {
        res
    }
}
