use std::net::TcpListener;

#[test]
pub fn test_get_from_single_peer() {
    // todo: specify 0 port so that os can choose a port.

    let localhost = "127.0.0.1:0";

    // let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    // and maybe close? kinda sucks.
}
