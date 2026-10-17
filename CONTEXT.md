# HealthCheck CLI

The `healthcheck` command-line tool for the HealthCheck REST API, and the tests that prove it works against HealthCheck.

## Language

### Credentials

**Personal token**:
A credential a person creates on the API & Agent Access page of their account and puts in `HEALTHCHECK_TOKEN`. It always wins over a stored sign-in, and `logout` leaves it alone.
_Avoid_: API key

**Agent connection**:
The stored sign-in that `healthcheck login` creates through Auth. It acts as the person, in one Account at a time, and the CLI renews it by itself.
_Avoid_: Session

### Commands

**Generated command**:
A `healthcheck <group> <action>` command the CLI builds when it runs, from one operation in the host's OpenAPI description. It never needs a new CLI release.
_Avoid_: Dynamic command

**Hand-written command**:
A command compiled into the CLI: `login`, `logout`, `status`, `accounts`, `api`, and `skill`. It never needs the description.
_Avoid_: Built-in

### Testing

**Fake host**:
A stand-in for HealthCheck or Auth that a test starts inside its own process and scripts request by request. It is used by every test.
_Avoid_: Mock server, stub
