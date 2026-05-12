# Happy Eyes

A Rust implementation of Happy Eyeballs v2 (RFC 8305) for fast dual-stack connections.

## Overview

Happy Eyes implements the Happy Eyeballs v2 algorithm as specified in [RFC 8305](https://datatracker.ietf.org/doc/html/rfc8305), which enables applications to connect quickly in dual-stack environments by racing IPv4 and IPv6 connections.

## Features

- **Concurrent DNS Resolution**: Simultaneous AAAA and A record queries
- **Resolution Delay**: 50ms IPv6 preference delay when A records arrive first
- **Connection Racing**: Staggered connection attempts with proper timing
- **IPv6 Preference**: Maintains IPv6 priority while providing IPv4 fallback

## If you would like to test:
```bash
git clone https://github.com/AlethaLabs/happy-eyes
cd happy-eyes
cargo run
```
The application will test connections to several well-known hosts and display timing metrics.

## Blog Post
If you are interested in learning more about the code in detail please visit my blog:
[AlethaLabs Blog](https://alethalabs.com/2025/10/12/racing-ip-for-happy-eyeballs/)

## Why did I make this
This is purely a learning project for me, I learned a ton about asynchronous programming in rust, and I am happy with the output. This isn't a full implementation of RFC 8305, but instead implements the core neccesities of racing IPs for connection. If you have any thoughts or see any errors I have made, please reach out! 

## How It Works

1. **DNS Phase**: Starts both IPv6 (AAAA) and IPv4 (A) queries concurrently
2. **Resolution Delay**: If A records complete first, waits 50ms for AAAA records
3. **Address Sorting**: Applies RFC 6724 destination address selection
4. **Connection Racing**: Attempts connections with staggered timing delays

## Dependencies
See [Cargo.toml](Cargo.toml)

## License

MIT License - See [LICENSE](LICENSE) file for details.

## References

- [RFC 8305: Happy Eyeballs Version 2](https://datatracker.ietf.org/doc/html/rfc8305)
- [RFC 6724: Default Address Selection for IPv6](https://datatracker.ietf.org/doc/html/rfc6724)
