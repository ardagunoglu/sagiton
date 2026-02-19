# Sagiton

Discord like application built with Rust, due to the Turkish DNS block. 

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

### Milestone 0 – Project Foundation (Completed)

- Rust workspace and backend skeleton created
- Clean Architecture folder structure established
- Docker Compose configuration (PostgreSQL + Redis)
- Environment configuration via `.env`
- Health endpoint implemented
- Integration test infrastructure set up

### Milestone 1 – Authentication & Sessions (Completed)

Implemented and fully integration-tested:

- User registration
- Login
- Access JWT issuance
- Refresh token rotation
- Logout (session revocation)
- Current user endpoint (`/me`)
- Secure password hashing using Argon2id + salt + pepper
- Refresh token hashing with database persistence

All auth endpoints and health checks are covered by integration tests.

## Next Milestone
### Milestone 2 – Guild & Channel Core

Planned work:
- Guild (server) model
- Channel model
- Membership system
- Role based authorization groundwork
- Guild/channel CRUD endpoints
- Integration tests for all new endpoints
- Permission checks integrated into service layer
- This milestone establishes the structural foundation for real time messaging and presence features.

## License

Copyright (C) 2026 Kadir Arda Günoğlu

Sagiton is licensed under the GNU General Public License v3.0 (GPL-3.0).

This means:

- You are free to use, study, modify, and distribute this software.
- Any derivative work must also be licensed under GPL-3.0.
- You cannot convert this project into a closed source or proprietary product.

Contributions are welcome via pull requests. By contributing, you agree that your contributions will be licensed under GPL-3.0.
