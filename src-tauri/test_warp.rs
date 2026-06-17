#[tokio::main]
async fn main() {
    let route = warp::any().map(|| "Hello");
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let res = warp::serve(route).try_bind_with_graceful_shutdown(([127, 0, 0, 1], 3335), async { rx.await.ok(); });
    match res {
        Ok(_) => println!("Ok"),
        Err(_) => println!("Err"),
    }
}
