//! Default DHT bootstrap nodes and TCP relays (from https://nodes.tox.chat).

/// `(host, udp_port, public_key_hex)` — most nodes also run a TCP relay on the
/// same port.
pub const DEFAULT_BOOTSTRAP_NODES: &[(&str, u16, &str)] = &[
    (
        "144.217.167.73",
        33445,
        "7E5668E0EE09E19F320AD47902419331FFEE147BB3606769CFBE921A2A2FD34C",
    ),
    (
        "tox.abilinski.com",
        33445,
        "10C00EB250C3233E343E2AEBA07115A5C28920E9C8D29492F6D00B29049EDC7E",
    ),
    (
        "205.185.115.131",
        53,
        "3091C6BEB2A993F1C6300C16549FABA67098FF3D62C6D253828B531470B53D68",
    ),
    (
        "tox1.mf-net.eu",
        33445,
        "B3E5FA80DC8EBD1149AD2AB35ED8B85BD546DEDE261CA593234C619249419506",
    ),
];
