---
name: healthcheck
description: Use the `healthcheck` command-line tool to read and change a HealthCheck Account (Projects, URL checks, Availability, smoke tests, ignored URLs, telemetry, and the rest of what HealthCheck monitors) through its REST API. Use it when a person asks you to look something up in HealthCheck, change it, or start a smoke test. Every command prints JSON.
---

# healthcheck

`healthcheck` calls the HealthCheck REST API at `https://healthcheck.mvdm.io` for the person you work for. It acts
as that person, in one of their Accounts at a time. Every command prints JSON on stdout, and the exit code says
what happened.

Use it when the person asks about or wants to change anything they keep in HealthCheck, such as a Project, a URL
check, or a smoke test. Prefer a command over raw HTTP: the CLI signs in, builds the URL and body, and retries on
a rate limit.

Install, or upgrade, with:

```sh
curl -LsSf https://healthcheck.mvdm.io/cli/install.sh | sh
```

On Windows: `powershell -ExecutionPolicy ByPass -c "irm https://healthcheck.mvdm.io/cli/install.ps1 | iex"`.

## Sign in

Run `healthcheck status` first. It prints `"signedIn": true` or `false`, the credential in use, and the Account
commands act in; `user` is `null` with a Personal token. Without a credential it exits 0. With one that no longer
works it exits 1.

- `healthcheck login` opens the person's browser at Auth. The person signs in and allows the CLI. You wait; the
  command ends when they have.
- `healthcheck login --device` prints a link with the code filled in, and the code, on stderr. Show both to the
  person; they finish on any device. `login` falls back to this by itself when it cannot open a browser, such
  as over SSH.
- `HEALTHCHECK_TOKEN` holds a Personal token for unattended runs, such as CI. It wins over the stored sign-in.
  Only the person can create one, on the API & Agent Access page of their account. Never ask them to paste one
  to you; ask them to set the variable.
- `HEALTHCHECK_URL` points the CLI at another HealthCheck host. Leave it unset for `https://healthcheck.mvdm.io`.
- `healthcheck logout` ends this CLI's stored sign-in at Auth and forgets it; `HEALTHCHECK_TOKEN` stays. Do not run
  it unless the person asks.

A command that fails with `not-signed-in` needs `healthcheck login`.

## Accounts

- `healthcheck accounts list` lists the person's Accounts. The one commands act in has `"current": true`.
- `healthcheck accounts switch <id>` moves the credential into another Account, for every later command. Switch
  only when the person asks for another Account, and say which one you switched to.

## Commands

Every REST operation is a command: `healthcheck <group> <action>`, both in kebab-case, from its `operationId`
`<group>.<action>`. The operation `projects.start-smoke-test` is `healthcheck projects start-smoke-test <projectId> <id>`.
The group comes from the `operationId`, not the tag: every Project command, including the URL check and smoke test
ones, is in the `projects` group.

- `healthcheck --help` lists the groups. `healthcheck <group> --help` lists its actions.
  `healthcheck <group> <action> --help` shows what the operation does and every argument with its type. Read it
  before your first call of an action.
- Path parameters are positional arguments, in path order.
- Query parameters are options, kebab-cased. A list action pages with `--page` and `--page-size`, as in
  `healthcheck projects list-run-pages 7 12 40 --page 2 --page-size 50`.
- Top-level body fields are options, kebab-cased. A list option repeats.

  ```sh
  healthcheck projects create --name "Web shop"
  healthcheck projects create-url-check 7 --name Home --url https://example.com
  healthcheck projects add-ignored-url 7 12 --url https://example.com/admin
  healthcheck projects start-smoke-test 7 12
  ```

- `--body '<json>'` or `--body @file.json` sends a whole body, for nested shapes. Field options win over the same
  keys in it.
- `healthcheck api <method> <path> [--body …] [--out …]` makes one raw call, for anything the commands do not
  shape well: `healthcheck api GET '/api/v1/projects'`.

A group or action the CLI does not know makes it fetch the API's description again, so a new operation works at
once. A command that is still unknown is a usage mistake.

## Output and exit codes

| Code | Meaning | What to do |
| ---- | ------- | ---------- |
| 0 | Success. stdout holds the JSON result. | Read the result. |
| 1 | Error. stderr holds the API's ProblemDetails, or `{"error":"<code>","message":…}`. | Read `title`, `detail`, or `message`, fix the cause or tell the person. |
| 2 | Usage mistake. | Fix your command line; check `--help`. |

On a rate limit (429) the CLI waits and retries by itself, up to three times. Do not retry an exit 1 in a loop.

## Files

An action that answers a file needs `--out <path>`. The CLI writes the file there and prints
`{"path","bytes","contentType","fileName"}`. It never writes file bytes to stdout.

```sh
healthcheck projects download-run-page-screenshot 7 12 40 3 8d1e2f4a-1b2c-4d5e-8f60-718293a4b5c6 --out shot.png
```

## Privacy

The CLI talks only to the HealthCheck host and its Auth. It sends no usage data.
