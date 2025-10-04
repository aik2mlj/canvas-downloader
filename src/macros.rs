#[macro_export]
macro_rules! fork {
    // Motivation: recursive async functions are unsupported. We avoid this by using a non-async
    // function `f` to tokio::spawn our recursive function. Conveniently, we can wrap our barrier logic in this function
    ($f:expr, $arg:expr, $T:ty, $options:expr) => {{
        fn g(arg: $T, options: std::sync::Arc<$crate::canvas::ProcessOptions>) {
            options.n_active_requests.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            tokio::spawn(async move {
                let _sem = options.sem_requests.acquire().await.unwrap_or_else(|e| {
                    panic!("Please report on GitHub. Unexpected closed sem, err={e}")
                });
                let res = $f(arg, options.clone()).await;
                let new_val = options.n_active_requests.fetch_sub(1, std::sync::atomic::Ordering::AcqRel) - 1;
                if new_val == 0 {
                    options.notify_main.notify_one();
                }
                if let Err(e) = res {
                    eprintln!("{e:?}");
                }
            });
        }
        g($arg, $options);
    }};
}
