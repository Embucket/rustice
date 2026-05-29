# Incident Response Plan

This plan applies to security incidents affecting Rustice and the Rustice
Snowflake Native App with Snowpark Container Services.

## Security Contacts

Report suspected security incidents privately to:

- artem@embucket.com
- cam@embucket.com

Do not open public GitHub issues for suspected vulnerabilities, credential
exposure, malware findings, or consumer-impacting security events.

## Severity Levels and SLAs

Severity is assigned from the maximum credible impact to consumer data,
credentials, service availability, or provider release artifacts.

| Severity | Examples | Initial response target | Remediation target |
| --- | --- | --- | --- |
| Critical | Confirmed credential exposure, remote code execution in the runtime image, unauthorized access to consumer data, malware in a release image | 1 business day | 7 calendar days |
| High | Exploitable vulnerability in a reachable runtime path, bypass of SPCS ingress/authz assumptions, high-impact dependency CVE in the final runtime image | 2 business days | 30 calendar days |
| Medium | Vulnerability requiring unusual privileges or limited impact, incomplete hardening control, non-blocking security misconfiguration | 5 business days | 90 calendar days |
| Low | Defense-in-depth issue, documentation gap, low-impact dependency advisory, non-reachable vulnerable code path | 10 business days | Next planned maintenance window |

Critical and High findings in the final Native App runtime image block release
unless a documented exception with compensating controls is approved by the
maintainers.

## Response Process

1. Intake and acknowledgement
   - Confirm receipt with the reporter.
   - Assign an incident owner.
   - Preserve reports, logs, scan output, affected commit SHAs, image digests,
     and Snowflake application package/version identifiers.

2. Triage
   - Classify severity using the table above.
   - Identify affected components: source code, container image, Native App
     package, SPCS service configuration, secret/reference flow, external access
     integration, or documentation.
   - Determine whether consumer data, credentials, keys, or service logs may be
     exposed.

3. Containment
   - Stop or suspend affected SPCS services when needed.
   - Revoke or rotate affected Snowflake secrets, PATs, references, or service
     user credentials when applicable.
   - Disable or supersede affected Native App versions/patches when needed.
   - Block release if CI image scanning finds High or Critical runtime image
     vulnerabilities.

4. Remediation
   - Patch code, dependency versions, Dockerfile/base image, Native App package
     SQL, service specification, or documentation as appropriate.
   - Rebuild the final runtime image.
   - Rerun vulnerability and malware scans.
   - Publish a patched Native App version or patch when a release artifact is
     affected.

5. Communication
   - Notify affected consumers and Snowflake support for material Native App
     issues.
   - Provide impact, affected versions, mitigation steps, and patched version or
     patch information.
   - Do not disclose sensitive exploit details publicly before a fix or
     mitigation is available.

6. Post-incident review
   - Record root cause, affected controls, timeline, remediation, and prevention
     actions.
   - Update the threat model, security docs, CI checks, or release process when
     the incident identifies a control gap.

## Evidence Retention

For security review and incident follow-up, maintainers retain:

- source commit and pull request references;
- final image digest and application package/version identifiers;
- Grype vulnerability reports;
- ClamAV malware scan reports;
- relevant CI logs and Snowflake service logs;
- remediation PRs and release notes.
