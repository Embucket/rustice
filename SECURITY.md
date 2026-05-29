# Security Policy

## Supported Scope

This policy applies to the Rustice codebase and the Snowflake Native App / SPCS
packaging under `deploy/native-app`.

For Native App distribution, the supported release artifact is the container
image built from the repository `Dockerfile` and published through Snowflake's
Native App package flow.

## Reporting Security Issues

Please report suspected vulnerabilities or security incidents privately to:

- artem@embucket.com
- cam@embucket.com

Do not open a public GitHub issue for sensitive security reports. Include the
affected component, reproduction steps when available, and any relevant logs or
scan findings.

## SDLC Security Practices

Rustice maintainers use the following controls for code and Native App changes:

- Source code is stored in GitHub and requires authenticated access for writes.
- Changes are reviewed through pull requests before merge.
- CI runs formatting, linting, and Rust test workflows on relevant changes.
- Rust static analysis is performed through `cargo clippy`.
- Native App threat modeling is documented in
  `security/THREAT_MODEL.md` and reviewed when package permissions, ingress,
  secrets, egress, or runtime image behavior changes.
- The Native App container image is built from a multi-stage Dockerfile and uses
  a statically linked Rust binary in a Debian 13 distroless static runtime image.
- The runtime image avoids Debian `libc6` runtime packages.
- The runtime container runs as `nonroot`.
- The runtime image does not download or execute additional application code at
  startup.
- Native App credentials are provided by Snowflake secrets or references at
  runtime and are not baked into the image.
- Native App egress is configured through Snowflake External Access Integration
  network rules.

## Image Security Scans

Before Native App publication or resubmission for Snowflake review, maintainers
run the image security workflow in `.github/workflows/image-security.yml`.

The workflow builds the final Docker image and performs:

- vulnerability scanning with Grype and upload of the full vulnerability report;
- a CI gate that fails on `HIGH` or `CRITICAL` findings;
- malware scanning with ClamAV over the exported final image root filesystem;
- upload of scan reports as GitHub Actions artifacts.

Reports should be retained with the release evidence used for Snowflake Native
App security review.

## Vulnerability Management

Vulnerabilities are triaged by severity, exploitability, affected component, fix
availability, and whether the affected code path is reachable in the Native App
runtime.

Target remediation timelines for applicable vulnerabilities in supported Native
App release artifacts:

- Critical: 7 calendar days
- High: 30 calendar days
- Medium: 90 calendar days
- Low: addressed opportunistically or during routine maintenance

Critical and High findings in the final runtime image block release until they
are remediated or formally accepted with documented compensating controls.
Non-fixable findings from the runtime base image are tracked in the scan report
and re-evaluated on each release or base image rebuild.

If a finding is not applicable, maintainers document the rationale in the release
or security review evidence. Examples include build-only dependencies that are
not present in the runtime image, unreachable code paths, or findings without a
fix where compensating controls apply.

When remediation requires a new runtime image, maintainers rebuild the image,
rerun the security scans, and publish a patched Native App version or patch.

## Incident Response

The published incident response plan is documented in
`security/INCIDENT_RESPONSE.md`.

Security incidents are triaged through the private contacts listed above.
Maintainers assess scope, impacted artifacts, consumer exposure, and required
remediation. The incident response plan defines severity levels, initial
response targets, remediation SLAs, containment steps, communication, and
post-incident review. For material Native App issues, consumers and Snowflake
support are notified with remediation guidance and patched release information.
