<!-- uuid: b2973dbd-0d7a-4952-830e-7cde45ff9042 -->
<!-- source: https://slsa.dev/spec/v1.2/threats | version: v1.2 (final, Nov 2025) | scraped: 2026-05-12 | tool: firecrawl v1.10.0 | re-pull: per REGISTRY.md policy -->
<!-- gate: [S] -->

[![SLSA logo](https://slsa.dev/images/logo.svg)](https://slsa.dev/)

- [Home](https://slsa.dev/)
- [Current activities](https://slsa.dev/current-activities)
- [SLSA v1.2](https://slsa.dev/spec/v1.2/)  - Understanding SLSA    - [What's new](https://slsa.dev/spec/v1.2/whats-new)
    - [About SLSA](https://slsa.dev/spec/v1.2/about)
    - [Supply chain threats](https://slsa.dev/spec/v1.2/threats-overview)
    - [Use cases](https://slsa.dev/spec/v1.2/use-cases)
    - [Guiding principles](https://slsa.dev/spec/v1.2/principles)
    - [FAQ](https://slsa.dev/spec/v1.2/faq)
    - [Future directions](https://slsa.dev/spec/v1.2/future-directions)
    - [Tracks](https://slsa.dev/spec/v1.2/tracks)
  - Build Track    - [Basics](https://slsa.dev/spec/v1.2/build-track-basics)
    - [Terminology](https://slsa.dev/spec/v1.2/terminology)
    - [Producing artifacts](https://slsa.dev/spec/v1.2/build-requirements)
    - [Distributing provenance](https://slsa.dev/spec/v1.2/distributing-provenance)
    - [Verifying artifacts](https://slsa.dev/spec/v1.2/verifying-artifacts)
    - [Assessing build platforms](https://slsa.dev/spec/v1.2/assessing-build-platforms)
  - Source Track    - [Producing source](https://slsa.dev/spec/v1.2/source-requirements)
    - [Verifying source](https://slsa.dev/spec/v1.2/verifying-source)
    - [Assessing source control systems](https://slsa.dev/spec/v1.2/assessing-source-systems)
    - [Example controls](https://slsa.dev/spec/v1.2/source-example-controls)
  - Cross Track Information    - [Threats & mitigations](https://slsa.dev/spec/v1.2/threats)
    - [Verified Properties](https://slsa.dev/spec/v1.2/verified-properties)
  - Attestation formats    - [General model](https://slsa.dev/spec/v1.2/attestation-model)
    - [Provenance](https://slsa.dev/spec/v1.2/provenance)
    - [Build Provenance](https://slsa.dev/spec/v1.2/build-provenance)
    - [Verification Summary](https://slsa.dev/spec/v1.2/verification_summary)
  - [Single-page view](https://slsa.dev/spec/v1.2/zonepage)
- [SLSA v1.1](https://slsa.dev/spec/v1.1/)
- [SLSA Working Draft](https://slsa.dev/spec/draft/)
- [How to SLSA](https://slsa.dev/how-to/)
- [Specification stages](https://slsa.dev/spec-stages)
- [Community](https://slsa.dev/community)
- [Blog](https://slsa.dev/blog)

[![SLSA logo](https://slsa.dev/images/logo.svg)](https://slsa.dev/)

# Threats & mitigations

Status: [Approved](https://slsa.dev/spec-stages)

On this page

- [Overview](https://slsa.dev/spec/v1.2/threats#overview)
- [Source threats](https://slsa.dev/spec/v1.2/threats#source-threats)
  - [(A) Producer](https://slsa.dev/spec/v1.2/threats#a-producer)
  - [(B) Modifying the source](https://slsa.dev/spec/v1.2/threats#b-modifying-the-source)
  - [(C) Source code management](https://slsa.dev/spec/v1.2/threats#c-source-code-management)
- [Build threats](https://slsa.dev/spec/v1.2/threats#build-threats)
  - [(D) External build parameters](https://slsa.dev/spec/v1.2/threats#d-external-build-parameters)
  - [(E) Build process](https://slsa.dev/spec/v1.2/threats#e-build-process)
  - [(F) Artifact publication](https://slsa.dev/spec/v1.2/threats#f-artifact-publication)
  - [(G) Distribution channel](https://slsa.dev/spec/v1.2/threats#g-distribution-channel)
- [Usage threats](https://slsa.dev/spec/v1.2/threats#usage-threats)
  - [(H) Package selection](https://slsa.dev/spec/v1.2/threats#h-package-selection)
  - [(I) Usage](https://slsa.dev/spec/v1.2/threats#i-usage)
- [Dependency threats](https://slsa.dev/spec/v1.2/threats#dependency-threats)
  - [Build dependency](https://slsa.dev/spec/v1.2/threats#build-dependency)
  - [Related threats](https://slsa.dev/spec/v1.2/threats#related-threats)
- [Availability threats](https://slsa.dev/spec/v1.2/threats#availability-threats)
- [Verification threats](https://slsa.dev/spec/v1.2/threats#verification-threats)

What follows is a comprehensive technical analysis of supply chain threats and
their corresponding mitigations with SLSA and other best practices. For an
introduction to the supply chain threats that SLSA is aiming to protect
against, see [Supply chain threats](https://slsa.dev/spec/v1.2/threats-overview).

The examples on this page are meant to:

- Explain the reasons for each of the SLSA [build](https://slsa.dev/spec/v1.2/build-requirements) and
[source](https://slsa.dev/spec/v1.2/source-requirements) requirements.
- Increase confidence that the SLSA requirements are sufficient to achieve the
desired [level](https://slsa.dev/spec/v1.2/about#how-slsa-works) of integrity protection.
- Help implementers better understand what they are protecting against so that
they can better design and implement controls.

## Overview

![Supply Chain Threats](https://slsa.dev/spec/v1.2/images/supply-chain-threats.svg)

This threat model covers the _software supply chain_, meaning the process by
which software is produced and consumed. We describe and cluster threats based
on where in the software development pipeline those threats occur, labeled (A)
through (I). This is useful because priorities and mitigations mostly cluster
along those same lines. Keep in mind that dependencies are
[highly recursive](https://slsa.dev/spec/v1.2/threats#dependency-threats), so each dependency has its own threats
(A) through (I), and the same for _their_ dependencies, and so on. For a more
detailed explanation of the supply chain model, see
[Terminology](https://slsa.dev/spec/v1.2/terminology).

Importantly, producers and consumers face _aggregate_ risk across all of the
software they produce and consume, respectively. Many organizations produce
and/or consume thousands of software packages, both first- and third-party, and
it is not practical to rely on every individual team in the organization to do
the right thing. For this reason, SLSA prioritizes mitigations that can be
broadly adopted in an automated fashion, minimizing the chance of mistakes.

## Source threats

A source integrity threat is a potential for an adversary to introduce a change
to the source code that does not reflect the intent of the software producer.
This includes modification of the source data at rest as well as insider threats,
when an authorized individual introduces an unauthorized change.

The SLSA Source track mitigates these threats when the consumer
[verifies source](https://slsa.dev/spec/v1.2/verifying-source) against expectations, confirming
that the revision they received was created in the expected manner.

### (A) Producer

The producer of the software intentionally produces code that harms the
consumer, or the producer otherwise uses practices that are not deserving of the
consumer’s trust.

Software producer intentionally creates a malicious revision of the source

_Threat:_ A producer intentionally creates a malicious revision with the intent of harming their consumers.

_Mitigation:_
This kind of attack cannot be directly mitigated through SLSA controls.
Consumers must establish some basis to trust the organizations from which they consume software.
That basis may be:

- The repo is open source with an active user-base. High numbers of engaged users may increase the likelihood that bad code is detected during code review and reduce the time-to-detection when bad revisions are accepted.
- The organization has sufficient legal or reputational incentives to dissuade it from making malicious changes.

Ultimately this is a judgement call with no straightforward answer.

_Example:_ A producer with an otherwise good reputation decides suddenly to produce a malicious artifact with the intent to harm their consumers.

### (B) Modifying the source

An adversary without any special administrator privileges attempts to introduce a change counter to the declared intent of the source by following the producer’s official source control process.

Threats in this category can be mitigated by following source control management best practices.

#### (B1) Submit change without review

Directly submit without review(Source L4)

_Threat:_ Malicious code submitted to the source repository.

_Mitigation:_ Require approval of all changes before they are accepted.

_Example:_ Adversary directly pushes a change to a git repo’s `main` branch.
Solution: The Source Control System is configured to require two party review for
contributions to the `main` branch.

Single actor controls multiple accounts

_Threat:_ An actor is able to control multiple account and effectively approve their own code changes.

_Mitigation:_ The producer must ensure that no actor is able to control or influence multiple accounts with review privileges.

_Example:_ Adversary creates a pull request using a secondary account and approves it using their primary account.
Solution: The producer must track all actors who have both explicit review permissions and the independent ability to control
a privileged bot. A common vector for this attack is to influence a robot account with the permission to review or contribute
code. Control of the robot account and an actor’s own personal account is enough to exploit this vulnerability. A common
solution to this flow is to deny bot accounts from contributing or reviewing code, or to require more human reviews in those
cases.

Use a robot account to submit change

_Threat:_ Exploit a robot account that has the ability to submit changes without
two-person review.

_Mitigation:_ All changes require review by two people, even changes authored by
robots.

_Example:_ A file within the source repository is automatically generated by a
robot, which is allowed to submit without review.
Adversary compromises the robot and submits a malicious change.
Solution: Require two-person review for such changes, ignoring the robot.

Abuse of rule exceptions

_Threat:_ Rule exceptions provide vector for abuse.

_Mitigation:_ Remove rule exceptions.

_Example:_ A producer intends to require two-person review on “all changes except for documentation changes,” defined as those only modifying `.md` files.
Adversary submits a malicious executable masquerading as a documentation file, `help.md`.
This avoids the two-person review rule due to the exception.
In the future, a user (or another workflow) can be induced to _execute_`help.md` and become compromised.
Technically the malicious code change met all defined policies yet the intent of the organization was defeated.
Solution: The producer adjusts the rules to prohibit such exceptions.

Highly-permissioned actor bypasses or disables controls(verification)

_Threat:_ Trusted actor with “admin” privileges in a repository submits code by disabling existing controls.

_Mitigation:_ The Source Control System must have controls in place to prevent
and detect abusive behavior from administrators (e.g. two-person approvals,
audit logging).

_Example:_ GitHub repository-level admin removes a branch protection requirement, pushes
their change, then re-enables the requirement to cover their tracks.
Solution: Consumers do not accept claims from the Source Control System unless
they trust sufficient controls are in place to prevent repo admins from
abusing privileges.

#### (B2) Evade change management process

Alter change history(Source L2+)

_Threat:_ Adversary alters branch history to hide malicious activity.

_Mitigation:_ The Source Control System prevents branch history from being
altered.

_Example:_ Adversary submits a malicious commit `X` to the `main` branch. A
release is built and published from `X`. The adversary then “force pushes”
to `main` erasing the record of the malicious commit. Solution: The Source
Control System is configured to prevent force pushes to `main`.

Replace tagged content with malicious content(Source L2+)

_Threat:_ Adversary alters a tag to point at malicious content.

_Mitigation:_ The Source Control System does not allow protected tags to be updated.

_Example:_ Adversary crafts a malicious commit `X` on a development branch which
does enforce any controls. They then update the `release_1.2` tag to point to
`X`. Consumers of `release_1.2` will get the malicious revision. Solution: The
Source Control System does not allow protected tags to be updated.

Skip required checks(Source L3+)

_Threat:_ Code is submitted without following the producers documented
development process, introducing unintended behavior.

_Mitigation:_ The producer uses the Source Control System to implement technical
controls ensuring adherence to the development process.

_Example:_ An engineer submits a new feature that has a critical flaw on an
untested code path, in violation of the producer’s documented process of having
high test coverage. Solution: The producer implements a technical control in the
SCS that requires 95%+ test coverage.

Modify code after review(Source L4)

_Threat:_ Modify the code after it has been reviewed but before submission.

_Mitigation:_ The Source Control System invalidates approvals whenever the proposed change is modified.

_Example:_ Source repository requires two-person review on all changes.
Adversary sends an initial “good” pull request to a peer, who approves it.
Adversary then modifies their proposal to contain “bad” code.

Solution: Configure the code review rules to require review of the most recent revision before submission.

Submit a change that is unreviewable(Source L4)

_Threat:_ Adversary crafts a change that looks benign to a reviewer but is actually malicious.

_Mitigation:_ Code review system ensures that all reviews are informed and
meaningful to the extent possible. For example the system could show
& resolve symlinks, render images, or verify & display provenance.

_Example:_ A proposed change updates a JPEG file to include a malicious
message, but the reviewer is only presented with a diff of the binary
file contents. The reviewer is unable to parse the contents themselves
so they do not have enough context to provide a meaningful review.
Solution: the code review system should present the reviewer with a
rendering of the image and the [embedded\\
metadata](https://en.wikipedia.org/wiki/Exif), allowing them to make an
informed decision.

Copy a reviewed change to another context(Source L4)

_Threat:_ Get a change reviewed in one context and then transfer it to a
different context.

_Mitigation:_ Approvals are context-specific.

_Example:_ MyPackage’s source repository requires two-person review. Adversary
forks the repo, submits a change in the fork with review from a colluding
colleague (who is not trusted by MyPackage), then proposes the change to
the upstream repo.
Solution: The proposed change still requires two-person review in the upstream
context even though it received two-person review in another context.

Commit graph attacks

_Threat:_ A malicious commit can be included in a sequence of commits such that it does not appear malicious in the net change presented to reviewers.

_Mitigation:_ The producer ensures that all revisions in the protected context followed the same contribution process.

_Example:_ Adversary sends a pull request containing malicious commit X and a commit Y that undoes X.
The combined change of X + Y displays zero lines of malicious code and the reviewer cannot tell that X is malicious unless they review it individually.
If X is allowed to become reachable from the protected branch, the content may become available in secured environments such as developer machines.

Solution: Each revision in the protected context must have followed the intended process.
Ultimately, this means that either each code review results in at most a single new commit or that the full process is followed for each constituent commit in a proposed sequence.

#### (B3) Render code review ineffective

Collude with another trusted person

_Threat:_ Two trusted persons collude to author and approve a bad change.

_Mitigation:_ This threat is not currently addressed by SLSA, but the producer can arbitrarily increase friction of their policies to reduce risk, such as requiring additional, or more senior reviewers.
The goal of policy here is to ensure that the approved changes match the intention of the producer for the source.
Increasing the friction of the policies may make it harder to circumvent, but doing so has diminishing returns.
Ultimately the producer will need to land upon a balanced risk profile that makes sense for their security posture.

Trick reviewer into approving bad code

_Threat:_ Construct a change that looks benign but is actually malicious, a.k.a.
“bugdoor.”

_Mitigation:_ This threat is not currently addressed by SLSA.

Reviewer blindly approves changes

_Threat:_ Reviewer approves changes without actually reviewing, a.k.a. “rubber
stamping.”

_Mitigation:_ This threat is not currently addressed by SLSA.

#### (B4) Render change metadata ineffective

Forge change metadata(Source L2+)

_Threat:_ Forge the change metadata to alter attribution, timestamp, or
discoverability of a change.

_Mitigation:_ The Source Control System only attributes changes to authenticated
identities and records contemporaneous evidence of changes in signed source
provenance attestations.

_Example:_ Adversary ‘X’ creates a commit with unauthenticated metadata claiming
it was authored by ‘Y’. Solution: The Source Control System records the identity
of ‘X’ when ‘X’ submits the commit to the repository.

### (C) Source code management

An adversary introduces a change to the source control repository through an
administrative interface, or through a compromise of the underlying
infrastructure.

Platform admin abuses privileges(verification)

_Threat:_ Platform administrator abuses their privileges to bypass controls or
to push a malicious version of the software.

_Mitigation:_ The source platform must have controls in place to prevent and
detect abusive behavior from administrators (e.g. two-person approvals for
changes to the infrastructure, audit logging). A future [Platform\\
Operations Track](https://slsa.dev/spec/v1.2/future-directions#platform-operations-track) may provide
more specific guidance on how to secure the underlying platform.

_Example 1:_ GitHostingService employee uses an internal tool to push changes to
the MyPackage source repo.

_Example 2:_ GitHostingService employee uses an internal tool to push a
malicious version of the server to serve malicious versions of MyPackage sources
to a specific CI/CD client but the regular version to everyone else, in order to
hide tracks.

_Example 3:_ GitHostingService employee uses an internal tool to push a
malicious version of the server that includes a backdoor allowing specific users
to bypass branch protections. Adversary then uses this backdoor to submit a
change to MyPackage without review.

_Solution:_ Consumers do not accept claims from the Source Control System unless
they trust sufficient controls are in place to prevent repo admins from
abusing privileges.

Exploit vulnerability in SCM

_Threat:_ Exploit a vulnerability in the implementation of the source code
management system to bypass controls.

_Mitigation:_ This threat is not currently addressed by SLSA.

## Build threats

A build integrity threat is a potential for an adversary to introduce behavior
to an artifact without changing its source code, or to build from a
source, dependency, and/or process that is not intended by the software
producer.

The SLSA Build track mitigates these threats when the consumer
[verifies artifacts](https://slsa.dev/spec/v1.2/verifying-artifacts) against expectations, confirming
that the artifact they received was built in the expected manner.

### (D) External build parameters

An adversary builds from a version of the source code that does not match the
official source control repository, or changes the build parameters to inject
behavior that was not intended by the official source.

The mitigation here is to compare the provenance against expectations for the
package, which depends on SLSA Build L1 for provenance. (Threats against the
provenance itself are covered by (E) and (F).)

Build from unofficial fork of code (expectations)

_Threat:_ Build using the expected CI/CD process but from an unofficial fork of
the code that may contain unauthorized changes.

_Mitigation:_ Verifier requires the provenance’s source location to match an
expected value.

_Example:_ MyPackage is supposed to be built from GitHub repo `good/my-package`.
Instead, it is built from `evilfork/my-package`. Solution: Verifier rejects
because the source location does not match.

Build from unofficial branch or tag (expectations)

_Threat:_ Build using the expected CI/CD process and source location, but
checking out an “experimental” branch or similar that may contain code not
intended for release.

_Mitigation:_ Verifier requires that the provenance’s source branch/tag matches
an expected value, or that the source revision is reachable from an expected
branch.

_Example:_ MyPackage’s releases are tagged from the `main` branch, which has
branch protections. Adversary builds from the unprotected `experimental` branch
containing unofficial changes. Solution: Verifier rejects because the source
revision is not reachable from `main`.

Build from unofficial build steps (expectations)

_Threat:_ Build the package using the proper CI/CD platform but with unofficial
build steps.

_Mitigation:_ Verifier requires that the provenance’s build configuration source
matches an expected value.

_Example:_ MyPackage is expected to be built by Google Cloud Build using the
build steps defined in the source’s `cloudbuild.yaml` file. Adversary builds
with Google Cloud Build, but using custom build steps provided over RPC.
Solution: Verifier rejects because the build steps did not come from the
expected source.

Build from unofficial parameters (expectations)

_Threat:_ Build using the expected CI/CD process, source location, and
branch/tag, but using a parameter that injects unofficial behavior.

_Mitigation:_ Verifier requires that the provenance’s external parameters all
match expected values.

_Example 1:_ MyPackage is supposed to be built from the `release.yml` workflow.
Adversary builds from the `debug.yml` workflow. Solution: Verifier rejects
because the workflow parameter does not match the expected value.

_Example 2:_ MyPackage’s GitHub Actions Workflow uses `github.event.inputs` to
allow users to specify custom compiler flags per invocation. Adversary sets a
compiler flag that overrides a macro to inject malicious behavior into the
output binary. Solution: Verifier rejects because the `inputs` parameter was not
expected.

Build from modified version of code modified after checkout (expectations)

_Threat:_ Build from a version of the code that includes modifications after
checkout.

_Mitigation:_ Build platform pulls directly from the source repository and
accurately records the source location in provenance.

_Example:_ Adversary fetches from MyPackage’s source repo, makes a local commit,
then requests a build from that local commit. Builder records the fact that it
did not pull from the official source repo. Solution: Verifier rejects because
the source repo does not match the expected value.

### (E) Build process

An adversary introduces an unauthorized change to a build output through
tampering of the build process; or introduces false information into the
provenance.

These threats are directly addressed by the SLSA Build track.

Forge values of the provenance (other than output digest) (Build L2+)

_Threat:_ Generate false provenance and get the trusted control plane to sign
it.

_Mitigation:_ At Build L2+, the trusted control plane [generates](https://slsa.dev/spec/v1.2/build-requirements#provenance-authentic) all
information that goes in the provenance, except (optionally) the output artifact
hash. At Build L3+, this is [hardened](https://slsa.dev/spec/v1.2/build-requirements#provenance-unforgeable) to prevent compromise even
by determined adversaries.

_Example 1 (Build L2):_ Provenance is generated on the build worker, which the
adversary has control over. Adversary uses a malicious process to get the build
platform to claim that it was built from source repo `good/my-package` when it
was really built from `evil/my-package`. Solution: Builder generates and signs
the provenance in the trusted control plane; the worker reports the output
artifacts but otherwise has no influence over the provenance.

_Example 2 (Build L3):_ Provenance is generated in the trusted control plane,
but workers can break out of the container to access the signing material.
Solution: Builder is hardened to provide strong isolation against tenant
projects.

Forge output digest of the provenance (n/a)

_Threat:_ The tenant-controlled build process sets output artifact digest
(`subject` in SLSA Provenance) without the trusted control plane verifying that
such an artifact was actually produced.

_Mitigation:_ None; this is not a problem. Any build claiming to produce a given
artifact could have actually produced it by copying it verbatim from input to
output.[1](https://slsa.dev/spec/v1.2/threats#fn1) (Reminder: Provenance is only a claim that a particular
artifact was _built_, not that it was _published_ to a particular registry.)

_Example:_ A legitimate MyPackage artifact has digest `abcdef` and is built
from source repo `good/my-package`. A malicious build from source repo
`evil/my-package` claims that it built artifact `abcdef` when it did not.
Solution: Verifier rejects because the source location does not match; the
forged digest is irrelevant.

Compromise project owner (Build L2+)

_Threat:_ An adversary gains owner permissions for the artifact’s build project.

_Mitigation:_ The build project owner must not have the ability to influence the
build process or provenance generation.

_Example:_ MyPackage is built on Awesome Builder under the project “mypackage”.
Adversary is an owner of the “mypackage” project. Awesome Builder allows
owners to debug the build environment via SSH. An adversary uses this feature
to alter a build in progress. Solution: Build L3 requires the external parameters
to be complete in the provenance. The attackers access and/or actions within the
SSH connection would be enumerated within the external parameters. The updated
external parameters will not match the declared expectations causing verification
to fail.

Compromise other build (Build L3)

_Threat:_ Perform a malicious build that alters the behavior of a benign
build running in parallel or subsequent environments.

_Mitigation:_ Builds are [isolated](https://slsa.dev/spec/v1.2/build-requirements#isolated) from one another, with no way for one to
affect the other or persist changes.

_Example 1:_ A build platform runs all builds for project MyPackage on
the same machine as the same Linux user. An adversary starts a malicious build
that listens for another build and swaps out source files, then starts a benign
build. The benign build uses the malicious build’s source files, but its
provenance says it used benign source files. Solution: The build platform
changes architecture to isolate each build in a separate VM or similar.

_Example 2:_ A build platform uses the same machine for subsequent
builds. An adversary first runs a build that replaces the `make` binary with a
malicious version, then subsequently runs an otherwise benign build. Solution:
The builder changes architecture to start each build with a clean machine image.

Steal cryptographic secrets (Build L3)

_Threat:_ Use or exfiltrate the provenance signing key or some other
cryptographic secret that should only be available to the build platform.

_Mitigation:_ Builds are [isolated](https://slsa.dev/spec/v1.2/build-requirements#isolated) from the trusted build platform control
plane, and only the control plane has [access](https://slsa.dev/spec/v1.2/build-requirements#provenance-unforgeable) to cryptographic
secrets.

_Example:_ Provenance is signed on the build worker, which the adversary has
control over. Adversary uses a malicious process that generates false provenance
and signs it using the provenance signing key. Solution: Builder generates and
signs provenance in the trusted control plane; the worker has no access to the
key.

Poison the build cache (Build L3)

_Threat:_ Add a malicious artifact to a build cache that is later picked up by a
benign build process ( [example](https://adnanthekhan.com/2024/05/06/the-monsters-in-your-build-cache-github-actions-cache-poisoning/)).

_Mitigation:_ Build caches must be [isolated](https://slsa.dev/spec/v1.2/build-requirements#isolated) between builds to prevent
such cache poisoning attacks. In particular, the cache SHOULD be keyed by the
transitive closure of all inputs to the cached artifact, and the cache must
either be only writable by the trusted control plane or have SLSA Build L3
provenance for each cache entry.

_Example 1:_ The cache key does not fully cover the transitive closure of all
inputs and instead only uses the digest of the source file itself. Adversary runs
a build over `auth.cc` with command line flags to gcc that define a marco
replacing `CheckAuth(ctx)` with `true`. When subsequent builds build `auth.cc`
they will get the attacker’s poisoned instance that does not call `CheckAuth`.
Solution: Build cache is keyed by digest of `auth.cc`, command line, and digest of
gcc so changing the command line flags results in a different cache entry.

_Example 2:_ The tenant controlled build process has full write access to the
cache. Adversary observes a legitimate build of `auth.cc` which covers the
transitive closure of all inputs and notes the digest used for caching. The
adversary builds a malicious version of `auth.o` and directly writes it to the
build cache using the observed digest. Subsequent legitimate builds will use
the malicious version of `auth.o`. Solution: Each cache entry is keyed by the
transitive closure of the inputs, and the cache entry is itself a SLSA Build L3
build with its own provenance that corresponds to the key.

Compromise build platform admin (verification)

_Threat:_ An adversary gains admin permissions for the artifact’s build platform.

_Mitigation:_ The build platform must have controls in place to prevent and
detect abusive behavior from administrators (e.g. two-person approvals, audit
logging).

_Example:_ MyPackage is built on Awesome Builder. Awesome Builder allows
engineers on-call to SSH into build machines to debug production issues. An
adversary uses this access to modify a build in progress. Solution: Consumers
do not accept provenance from the build platform unless they trust sufficient
controls are in place to prevent abusing admin privileges.

### (F) Artifact publication

An adversary uploads a package artifact that does not reflect the intent of the
package’s official source control repository.

This is the most direct threat because it is the easiest to pull off. If there
are no mitigations for this threat, then (D) and (E) are often indistinguishable
from this threat.

Build with untrusted CI/CD (expectations)

_Threat:_ Build using an unofficial CI/CD pipeline that does not build in the
correct way.

_Mitigation:_ Verifier requires provenance showing that the builder matched an
expected value.

_Example:_ MyPackage is expected to be built on Google Cloud Build, which is
trusted up to Build L3. Adversary builds on SomeOtherBuildPlatform, which is only
trusted up to Build L2, and then exploits SomeOtherBuildPlatform to inject
malicious behavior. Solution: Verifier rejects because builder is not as
expected.

Upload package without provenance (Build L1)

_Threat:_ Upload a package without provenance.

_Mitigation:_ Verifier requires provenance before accepting the package.

_Example:_ Adversary uploads a malicious version of MyPackage to the package
repository without provenance. Solution: Verifier rejects because provenance is
missing.

Tamper with artifact after CI/CD (Build L1)

_Threat:_ Take a benign version of the package, modify it in some way, then
re-upload it using the original provenance.

_Mitigation:_ Verifier checks that the provenance’s `subject` matches the hash
of the package.

_Example:_ Adversary performs a proper build, modifies the artifact, then
uploads the modified version of the package to the repository along with the
provenance. Solution: Verifier rejects because the hash of the artifact does not
match the `subject` found within the provenance.

Tamper with provenance (Build L2)

_Threat:_ Perform a build that would not meet expectations, then modify the
provenance to make the expectations checks pass.

_Mitigation:_ Verifier only accepts provenance with a valid [cryptographic\\
signature](https://slsa.dev/spec/v1.2/build-requirements#provenance-authentic) or equivalent proving that the provenance came from an
acceptable builder.

_Example:_ MyPackage is expected to be built by GitHub Actions from the
`good/my-package` repo. Adversary builds with GitHub Actions from the
`evil/my-package` repo and then modifies the provenance so that the source looks
like it came from `good/my-package`. Solution: Verifier rejects because the
cryptographic signature is no longer valid.

### (G) Distribution channel

An adversary modifies the package on the package registry using an
administrative interface or through a compromise of the infrastructure
including modification of the package in transit to the consumer.

The distribution channel threats and mitigations look very similar to the
Artifact Publication (F) threats and mitigations with the main difference
being that these threats are mitigated by having the _consumer_ perform
verification.

The consumer’s actions may be simplified if (F) produces a [VSA](https://slsa.dev/spec/v1.2/verification_summary).
In this case the consumer may replace provenance verification with
[VSA verification](https://slsa.dev/spec/v1.2/verification_summary#how-to-verify).

Build with untrusted CI/CD (expectations)

_Threat:_ Replace the package with one built using an unofficial CI/CD pipeline
that does not build in the correct way.

_Mitigation:_ Verifier requires provenance showing that the builder matched an
expected value or a VSA for corresponding `resourceUri`.

_Example:_ MyPackage is expected to be built on Google Cloud Build, which is
trusted up to Build L3. Adversary builds on SomeOtherBuildPlatform, which is only
trusted up to Build L2, and then exploits SomeOtherBuildPlatform to inject
malicious behavior. Adversary then replaces the original package within the
repository with the malicious package. Solution: Verifier rejects because
builder is not as expected.

Issue VSA from untrusted intermediary (expectations)

_Threat:_ Have an unofficial intermediary issue a VSA for a malicious package.

_Mitigation_: Verifier requires VSAs to be issued by a trusted intermediary.

_Example:_ Verifier expects VSAs to be issued by TheRepository. Adversary
builds a malicious package and then issues a VSA of their own for the malicious
package. Solution: Verifier rejects because they only accept VSAs from
TheRepository which the adversary cannot issue since they do not have the
corresponding signing key.

Upload package without provenance or VSA (Build L1)

_Threat:_ Replace the original package with a malicious one without provenance.

_Mitigation:_ Verifier requires provenance or a VSA before accepting the package.

_Example:_ Adversary replaces MyPackage with a malicious version of MyPackage
on the package repository and deletes existing provenance. Solution: Verifier
rejects because provenance is missing.

Replace package and VSA with another (expectations)

_Threat:_ Replace a package and its VSA with a malicious package and its valid VSA.

_Mitigation_: Consumer ensures that the VSA matches the package they’ve requested (not just the package they received) by following the [verification process](https://slsa.dev/spec/v1.2/verification_summary#how-to-verify).

_Example:_ Adversary uploads a malicious package to `repo/evil-package`,
getting a valid VSA for `repo/evil-package`. Adversary then replaces
`repo/my-package` and its VSA with `repo/evil-package` and its VSA.
Solution: Verifier rejects because the VSA `resourceUri` field lists
`repo/evil-package` and not the expected `repo/my-package`.

Tamper with artifact after upload (Build L1)

_Threat:_ Take a benign version of the package, modify it in some way, then
replace it while retaining the original provenance or VSA.

_Mitigation:_ Verifier checks that the provenance or VSA’s `subject` matches
the hash of the package.

_Example:_ Adversary performs a proper build, modifies the artifact, then
replaces the modified version of the package in the repository and retains the
original provenance. Solution: Verifier rejects because the hash of the
artifact does not match the `subject` found within the provenance.

Tamper with provenance or VSA (Build L2)

_Threat:_ Perform a build that would not meet expectations, then modify the
provenance or VSA to make the expectations checks pass.

_Mitigation:_ Verifier only accepts provenance or VSA with a valid [cryptographic\\
signature](https://slsa.dev/spec/v1.2/build-requirements#provenance-authentic) or equivalent proving that the provenance came from an
acceptable builder or the VSA came from an expected verifier.

_Example 1:_ MyPackage is expected to be built by GitHub Actions from the
`good/my-package` repo. Adversary builds with GitHub Actions from the
`evil/my-package` repo and then modifies the provenance so that the source looks
like it came from `good/my-package`. Solution: Verifier rejects because the
cryptographic signature is no longer valid.

_Example 2:_ Verifier expects VSAs to be issued by TheRepository. Adversary
builds a malicious package and then modifies the original VSA’s `subject`
field to match the digest of the malicious package. Solution: Verifier rejects
because the cryptographic signature is no longer valid.

## Usage threats

A usage threat is a potential for an adversary to exploit behavior of the
consumer.

### (H) Package selection

The consumer requests a package that it did not intend.

Dependency confusion

_Threat:_ Register a package name in a public registry that shadows a name used
on the victim’s internal registry, and wait for a misconfigured victim to fetch
from the public registry instead of the internal one.

_Mitigation:_ The mitigation is for the software producer to build internal
packages on a SLSA Level 2+ compliant build system and define expectations for
build provenance. Expectations must be verified on installation of the internal
packages. If a misconfigured victim attempts to install attacker’s package with
an internal name but from the public registry, then verification against
expectations will fail.

For more information see [Verifying artifacts](https://slsa.dev/spec/v1.2/verifying-artifacts)
and [Defender’s Perspective: Dependency Confusion and Typosquatting Attacks](https://slsa.dev/blog/2024/08/dep-confusion-and-typosquatting).

Typosquatting

_Threat:_ Register a package name that is similar looking to a popular package
and get users to use your malicious package instead of the benign one.

_Mitigation:_ This threat is not currently addressed by SLSA. That said, the
requirement to make the source available can be a mild deterrent, can aid
investigation or ad-hoc analysis, and can complement source-based typosquatting
solutions.

### (I) Usage

The consumer uses a package in an unsafe manner.

Improper usage

_Threat:_ The software can be used in an insecure manner, allowing an
adversary to compromise the consumer.

_Mitigation:_ This threat is not addressed by SLSA, but may be addressed by
efforts like [Secure by Design](https://www.cisa.gov/securebydesign).

## Dependency threats

A dependency threat is a potential for an adversary to introduce unintended
behavior in one artifact by compromising some other artifact that the former
depends on at build time. (Runtime dependencies are excluded from the model, as
[noted below](https://slsa.dev/spec/v1.2/threats#runtime-dep).)

Unlike other threat categories, dependency threats develop recursively through
the supply chain and can only be exploited indirectly. For example, if
application _A_ includes library _B_ as part of its build process, then a build
or source threat to _B_ is also a dependency threat to _A_. Furthermore, if
library _B_ uses build tool _C_, then a source or build threat to _C_ is also a
dependency threat to both _A_ and _B_.

This version of SLSA does not explicitly address dependency threats, but we
expect that a future version will. In the meantime, you can [apply SLSA\\
recursively](https://slsa.dev/spec/v1.2/verifying-artifacts#step-3-optional-check-dependencies-recursively) to your dependencies in order to reduce the risk of dependency
threats.

### Build dependency

An adversary compromises the target artifact through one of its build
dependencies. Any artifact that is present in the build environment and has the
ability to influence the output is considered a build dependency.

Include a vulnerable dependency (library, base image, bundled file, etc.)

_Threat:_ Statically link, bundle, or otherwise include an artifact that is
compromised or has some vulnerability, causing the output artifact to have the
same vulnerability.

_Example:_ The C++ program MyPackage statically links libDep at build time. A
contributor accidentally introduces a security vulnerability into libDep. The
next time MyPackage is built, it picks up and includes the vulnerable version of
libDep, resulting in MyPackage also having the security vulnerability.

_Mitigation:_ A future
[Dependency track](https://slsa.dev/current-activities#dependency-track) may
provide more comprehensive guidance on how to address more specfiic
aspects of this threat.

Use a compromised build tool (compiler, utility, interpreter, OS package, etc.)

_Threat:_ Use a compromised tool or other software artifact during the build
process, which alters the build process and injects unintended behavior into the
output artifact.

_Mitigation:_ This can be partially mitigated by treating build tooling,
including OS images, as any other artifact to be verified prior to use.
The threats described in this document apply recursively to build tooling
as do the mitigations and examples. A future
[Build Environment track](https://slsa.dev/current-activities#build-environment-track) may
provide more comprehensive guidance on how to address more specfiic
aspects of this threat.

_Example:_ MyPackage is a tarball containing an ELF executable, created by
running `/usr/bin/tar` during its build process. An adversary compromises the
`tar` OS package such that `/usr/bin/tar` injects a backdoor into every ELF
executable it writes. The next time MyPackage is built, the build picks up the
vulnerable `tar` package, which injects the backdoor into the resulting
MyPackage artifact. Solution: [apply SLSA recursively](https://slsa.dev/spec/v1.2/verifying-artifacts#step-3-optional-check-dependencies-recursively) to all build tools
prior to the build. The build platform verifies the disk image,
or the individual components on the disk image, against the associated
provenance or VSAs prior to running a build. Depending on where the initial
compromise took place (i.e. before/during vs _after_ the build of the build tool itself), the modified `/usr/bin/tar` will fail this verification.

Use a compromised runtime dependency during the build (for tests, dynamic linking, etc.)

_Threat:_ During the build process, use a compromised runtime dependency (such
as during testing or dynamic linking), which alters the build process and
injects unwanted behavior into the output.

**NOTE:** This is technically the same case as [Use a compromised build\\
tool](https://slsa.dev/spec/v1.2/threats#build-tool). We call it out to remind the reader that
[runtime dependencies](https://slsa.dev/spec/v1.2/threats#runtime-dep) can become build dependencies if they are
loaded during the build.

_Example:_ MyPackage has a runtime dependency on package Dep, meaning that Dep
is not included in MyPackage but required to be installed on the user’s machine
at the time MyPackage is run. However, Dep is also loaded during the build
process of MyPackage as part of a test. An adversary compromises Dep such that,
when run during a build, it injects a backdoor into the output artifact. The
next time MyPackage is built, it picks up and loads Dep during the build
process. The malicious code then injects the backdoor into the new MyPackage
artifact.

_Mitigation:_ In addition to all the mitigations for build tools, you can often
avoid runtime dependencies becoming build dependencies by isolating tests to a
separate environment that does not have write access to the output artifact.

### Related threats

The following threats are related to “dependencies” but are not modeled as
“dependency threats”.

Use a compromised dependency at runtime (modeled separately)

_Threat:_ Load a compromised artifact at runtime, thereby compromising the user
or environment where the software ran.

_Example:_ MyPackage lists package Dep as a runtime dependency. Adversary
publishes a compromised version of Dep that runs malicious code on the user’s
machine when Dep is loaded at runtime. An end user installs MyPackage, which in
turn installs the compromised version of Dep. When the user runs MyPackage, it
loads and executes the malicious code from Dep.

_Mitigation:_ N/A - SLSA’s
threat model does not explicitly model runtime dependencies. Instead, each
runtime dependency is considered a distinct artifact with its own threats.

## Availability threats

An availability threat is a potential for an adversary to deny someone from
reading a source and its associated change history, or from building a package.

SLSA does not currently address availability threats, though future versions might.

Delete the code

_Threat:_ Perform a build from a particular source revision and then delete that
revision or cause it to get garbage collected, preventing anyone from inspecting
the code.

_Mitigation:_ This threat is not currently addressed by SLSA.

A dependency becomes temporarily or permanently unavailable to the build process

_Threat:_ Unable to perform a build with the intended dependencies.

_Mitigation:_ This threat is not currently addressed by SLSA. That said, some
solutions to support hermetic and reproducible builds may also reduce the
impact of this threat.

De-list artifact

_Threat:_ The package registry stops serving the artifact.

_Mitigation:_ This threat is not currently addressed by SLSA.

De-list provenance

_Threat:_ The package registry stops serving the provenance.

_Mitigation:_ This threat is not currently addressed by SLSA.

## Verification threats

Threats that can compromise the ability to prevent or detect the supply chain
security threats above.

Tamper with recorded expectations

_Threat:_ Modify the verifier’s recorded expectations, causing the verifier to
accept an unofficial package artifact.

_Mitigation:_ Changes to recorded expectations requires some form of
authorization, such as two-party review.

_Example:_ The package ecosystem records its expectations for a given package
name in a configuration file that is modifiable by that package’s producer. The
configuration for MyPackage expects the source repository to be
`good/my-package`. The adversary modifies the configuration to also accept
`evil/my-package`, and then builds from that repository and uploads a malicious
version of the package. Solution: Changes to the recorded expectations require
two-party review.

Exploit cryptographic hash collisions

_Threat:_ Exploit a cryptographic hash collision weakness to bypass one of the
other controls.

_Mitigation:_ Choose secure algorithms when using cryptographic digests, such
as SHA-256.

_Examples:_ Attacker crafts a malicious file with the same MD5 hash as a target
benign file. Attacker replaces the benign file with the malicious file.
Solution: Only accept cryptographic hashes with strong collision resistance.

1. Technically this requires the artifact to be known to the
adversary. If they only know the digest but not the actual contents, they
cannot actually build the artifact without a [preimage attack](https://en.wikipedia.org/wiki/Preimage_attack) on the digest
algorithm. However, even still there are no known concerns where this is a
problem. [↩](https://slsa.dev/spec/v1.2/threats#fnref1)


[‹ Example controls](https://slsa.dev/spec/v1.2/source-example-controls) [Verified Properties ›](https://slsa.dev/spec/v1.2/verified-properties)

**SLSA is a cross-industry collaboration.**

© 2026 The Linux Foundation, under the terms of the [Community Specification License 1.0](https://github.com/slsa-framework/governance)

**Privacy statement**

We use [GoatCounter](https://goatcounter.com/) to help us improve our website by collecting and reporting information on how it's used.
We do not store advertising or tracking cookies. The information we collect does not identify anyone and does not track an individual's use of the site.

[View source on GitHub](https://github.com/slsa-framework/slsa/blob/releases/v1.2/spec/threats.md?plain=1)

This site is powered by [Netlify](https://www.netlify.com/)

[![SLSA logo](https://slsa.dev/images/logo.svg)](https://slsa.dev/)