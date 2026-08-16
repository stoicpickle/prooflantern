# Security policy

Proof Lantern is an experimental local tool and does not send project data over
the network. It does read project-authored YAML, JSON, and evidence paths, so
path containment, terminal restoration, and untrusted input handling are treated
as security-sensitive behavior.

Please do not open a public issue for a vulnerability. Use GitHub's **Report a
vulnerability** action in this repository's Security tab. Include the affected
version or commit, reproduction steps, and likely impact. You should receive an
initial response within seven days.

Only the latest commit on `main` is currently supported; no stable release line
has been declared yet.
