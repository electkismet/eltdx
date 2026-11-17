use std::io::ErrorKind as IoErrorKind;
use std::net::{Ipv4Addr, TcpListener};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use eltdx::TdxClient;

#[test]
fn concurrent_client_clones_close_the_same_owner() -> eltdx::Result<()> {
    let client = TdxClient::builder().build()?;
    let clients = [client.clone(), client.clone()];
    let barrier = Arc::new(Barrier::new(clients.len() + 1));
    let closers: Vec<_> = clients
        .into_iter()
        .map(|client| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                client.close()
            })
        })
        .collect();
    barrier.wait();

    for closer in closers {
        closer.join().expect("close thread exits")?;
    }
    client.close()?;
    Ok(())
}

#[test]
fn close_before_first_operation_does_not_connect() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    listener.set_nonblocking(true)?;
    let endpoint = listener.local_addr()?;
    let client = TdxClient::builder()
        .endpoints(vec![endpoint])
        .close_timeout(Duration::from_millis(100))
        .cleanup_timeout(Duration::from_millis(100))
        .build()?;

    client.close()?;
    assert!(matches!(client.clone().close(), Ok(())));
    let accept = listener.accept();
    assert!(matches!(accept, Err(error) if error.kind() == IoErrorKind::WouldBlock));
    Ok(())
}

#[test]
fn close_remains_terminal_for_every_client_clone() -> eltdx::Result<()> {
    let client = TdxClient::builder().build()?;
    let clone = client.clone();

    client.close()?;
    clone.close()?;
    assert!(matches!(clone.close(), Ok(())));
    Ok(())
}
