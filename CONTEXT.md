# HealthCheck CLI

The `healthcheck` command-line tool for the HealthCheck REST API, and the tests that prove it works against HealthCheck.

## Language

### Testing against HealthCheck

**Fake host**:
A stand-in for HealthCheck or Auth that a test starts inside its own process and scripts request by request. It is used by every test.
_Avoid_: Mock server, stub
