use agilang_framework_http::{Request, Response};

pub trait Middleware {
    fn handle(&self, request: Request, next: &dyn Fn(Request) -> Response) -> Response;
}

#[derive(Default)]
pub struct MiddlewareChain<'a> {
    middlewares: Vec<&'a dyn Middleware>,
}

impl<'a> MiddlewareChain<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, middleware: &'a dyn Middleware) {
        self.middlewares.push(middleware);
    }

    pub fn execute<F>(&self, request: Request, core_handler: F) -> Response
    where
        F: Fn(Request) -> Response + 'static,
    {
        self.execute_at(0, request, &core_handler)
    }

    fn execute_at(
        &self,
        index: usize,
        request: Request,
        core_handler: &dyn Fn(Request) -> Response,
    ) -> Response {
        if index < self.middlewares.len() {
            let middleware = self.middlewares[index];
            middleware.handle(request, &|req| {
                self.execute_at(index + 1, req, core_handler)
            })
        } else {
            core_handler(request)
        }
    }
}
