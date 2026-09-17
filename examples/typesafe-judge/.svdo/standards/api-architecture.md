# API Architecture Standard

- API handlers should be thin and delegate domain behavior to service or core modules.
- Request validation should happen before mutation.
- Error responses should be consistent with existing API error conventions.
- New code should avoid introducing cross-layer dependencies.
