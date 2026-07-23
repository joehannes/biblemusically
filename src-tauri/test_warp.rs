// deprecated???: standalone scratch file, not declared as a module anywhere (no `mod test_warp`
// in lib.rs/main.rs) and not part of the compiled crate. Looks like a one-off experiment for
// verifying warp's graceful-shutdown API before it was wired into oauth.rs/projects.rs. Safe to
// delete once confirmed unused. See TODOS.md.
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
