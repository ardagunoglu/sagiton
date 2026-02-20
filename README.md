# Sagiton

Backend structure for a Discord-like application built with Rust, due to the Turkish DNS block. 

## Purpose

Sagiton aims to provide a cleanly architected backend for a real time communication platform.  
Current focus is on authentication, authorization, and core text/presence infrastructure. Voice and WebRTC will be added in later milestones.

The project follows:
- Clean Architecture principles
- Clear separation of domain, application, infrastructure, and interface layers
- Secure authentication and authorization practices
- Test driven, integration tested development

---

## Current Status

Implemented milestones:
- Milestone 0: Project foundation, clean architecture skeleton, Docker setup, health checks
- Milestone 1: Authentication and session lifecycle (register/login/refresh/logout/me)
- Milestone 2: Guild and channel core (membership, channel management, permissions groundwork)
- Milestone 3: Realtime messaging platform

Milestone 3 highlights:
- Channel messaging and thread messaging (DM/group/guild channels)
- WebSocket gateway with JWT auth, heartbeat, subscribe model, and deterministic subscribe ack flow
- Redis pub/sub fanout with reconnect strategy and echo suppression for multi instance consistency
- Presence lifecycle with multi device refcount and TTL refresh
- Guild invite tokens and group invite tokens
- Unread/last_read baseline (`channel_reads`, `thread_reads`) with `has_unread` in list responses
- Friendship baseline (`friend_requests`, `friends`) with request/accept/reject/list flows
- Direct thread behavior: accepted friendship creates ACTIVE DM flow

Testing status:
- Integration tests are in place for auth, guild/channel, conversation, friendship, message, and ws realtime suites
- Endpoint level coverage is enforced for newly added APIs

## Planned Next

- OpenAPI/spec generation and API contract standardization
- Standardized error response model (problem style structure)
- Pagination contract hardening and cursor documentation
- Continued operational hardening for production rollout

## License

Copyright (C) 2026 Kadir Arda Günoğlu

Sagiton is licensed under the GNU General Public License v3.0 (GPL-3.0).

This means:

- You are free to use, study, modify, and distribute this software.
- Any derivative work must also be licensed under GPL-3.0.
- You cannot convert this project into a closed source or proprietary product.

Contributions are welcome via pull requests. By contributing, you agree that your contributions will be licensed under GPL-3.0.
