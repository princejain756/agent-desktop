use super::*;

fn pair() -> (Connection, Connection) {
    let (client, server) = std::os::unix::net::UnixStream::pair().unwrap();
    let (client, server) = zbus::block_on(futures_lite::future::zip(
        zbus::connection::Builder::async_io_unix_stream(client)
            .p2p()
            .build(),
        zbus::connection::Builder::async_io_unix_stream(server)
            .server(zbus::Guid::generate())
            .unwrap()
            .p2p()
            .build(),
    ));
    (client.unwrap(), server.unwrap())
}

#[test]
fn a_session_change_or_closed_transport_reopens_without_caching_failures() {
    let deadline = || Deadline::after(5_000).unwrap();
    let (old, _old_server) = pair();
    let (fresh, _fresh_server) = pair();
    let slot = Mutex::new(Some(CachedConnection {
        environment: vec![Some("another-session".into())],
        connection: Some(old.clone()),
    }));
    let connected = get(&slot, deadline(), |_| Ok(fresh.clone())).unwrap();
    assert_eq!(connected.server_guid(), fresh.server_guid());
    assert_ne!(connected.server_guid(), old.server_guid());
    forget(&slot, &old);
    assert_eq!(
        get(&slot, deadline(), |_| panic!("reuse live generation"))
            .unwrap()
            .server_guid(),
        fresh.server_guid()
    );
    zbus::block_on(connected.close()).unwrap();
    assert!(
        get(&slot, deadline(), |_| Err(AdapterError::internal(
            "open failed"
        )))
        .is_err()
    );
    assert!(slot.lock().unwrap().as_ref().unwrap().connection.is_none());
    let (replacement, _replacement_server) = pair();
    assert_eq!(
        get(&slot, deadline(), |_| Ok(replacement.clone()))
            .unwrap()
            .server_guid(),
        replacement.server_guid()
    );
    assert!(
        get(&slot, Deadline::after(0).unwrap(), |_| panic!(
            "expired budget"
        ))
        .is_err()
    );
    forget(&slot, &replacement);
    assert!(slot.lock().unwrap().as_ref().unwrap().connection.is_none());
}

#[test]
fn peek_returns_only_a_live_connection_for_the_current_environment() {
    let slot = Mutex::new(None);
    assert!(peek(&slot).is_none());

    let (live, _server) = pair();
    let foreign = Mutex::new(Some(CachedConnection {
        environment: vec![Some("another-session".into())],
        connection: Some(live.clone()),
    }));
    assert!(peek(&foreign).is_none(), "a stale environment hides it");

    let slot = Mutex::new(None);
    let _ = get(&slot, Deadline::after(5_000).unwrap(), |_| Ok(live.clone())).unwrap();
    assert_eq!(
        peek(&slot).unwrap().server_guid(),
        live.server_guid(),
        "a cached live connection is reused"
    );
    zbus::block_on(live.close()).unwrap();
    assert!(peek(&slot).is_none(), "a closed connection is not reused");
}
