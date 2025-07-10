# Autobahn

## What is this?

This is a lightweight decentralized publish-subscribe server similar to ROS that is designed to work with the autobahn client (available on GitHub).

### Under the hood

The server implements a publish-subscribe pattern over WebSockets, using Protocol Buffers to serialize and deserialize internal server messages. It automatically discovers other servers on the network using mDNS-SD and connects to them to relay messages between clients across different servers. This eliminates the need for a centralized server and reduces overhead.

Note that clients are responsible for establishing their own connections to the server. This implementation serves as a server node in the distributed network.

Please note that while this project's code could use improvement, it is functional and ready for use.

## Contributions Welcome as I'm not the most experienced rust dev :)

# Runtime

## Inside one computer

publish relay subscribe
client -> server -> client

Please note that most of these time constrains are due to the websockets and not the server itself (Ok sure it [server] adds like ~0.5ms at higher loads but still)

- <1ms for messages lower than 1mb
- ~2ms for messages of 1mb
- ~>2ms for messages of >1mb

---

## Build

```bash
docker build -t autobahn .
```

## Run

```bash
docker run -p 8080:8080 autobahn
```
