// bare async (no runtime chosen here — just syntax)
async fn fetch_user(id: i32) -> String {
    format!("user_{}", id)
}

async fn fetch_all(ids: Vec<i32>) -> Vec<String> {
    let mut out = Vec::new();
    for id in ids {
        out.push(fetch_user(id).await);
    }
    out
}

// async block
async fn parallel_calc(a: i32, b: i32) -> i32 {
    let fut = async move { a + b };
    fut.await
}

// generic async
async fn twice<F, T>(f: F) -> (T, T)
where
    F: Fn() -> T,
    T: Clone,
{
    let v = f();
    (v.clone(), v)
}
