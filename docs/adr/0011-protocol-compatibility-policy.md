# ADR 0011: Separate wire parsing, semantic support, and security policy

- Status: Accepted
- Date: 2026-11-08

## Context

IP Messenger is an extensible wire protocol. A packet may be syntactically valid while using a command, option bit, or command-specific extension that IPMsgSec does not implement. Treating every unsupported feature as malformed input conflates syntax with feature support and makes interoperability failures difficult to diagnose. Conversely, silently ignoring unknown options can change the meaning of a command and may cause an unsafe state transition.

The protocol core must remain bounded, deterministic, and suitable for Verus reasoning. It must also preserve enough wire information to support future compatibility work without guessing at unknown semantics.

## Decision

Separate packet handling into three layers:

1. **Wire parser:** validate the bounded common envelope and preserve raw fields. It does not decide feature support.
2. **Semantic classifier:** convert only understood command/option combinations into typed protocol events. Unknown commands and unsupported option combinations receive distinct errors.
3. **Security/state policy:** apply only typed events that satisfy state invariants. Rejected input does not mutate state.

The parser preserves the full command word, including command ID and option bits, and borrows the additional-data tail without lossy conversion. Unknown command IDs remain unsupported rather than being remapped.

For a known command, any option bits not implemented for that command produce `ProtocolError::UnsupportedOptions { command_id, flags }`. The flags are reported intact. This error is distinct from malformed packet syntax and from an unknown command. The current implementation does not silently ignore unknown or unimplemented option bits.

Command-specific data is parsed only by a command-specific bounded parser when its grammar is implemented. Until then:
- Discovery additional data may remain opaque and be ignored because the current discovery event does not derive behavior from it.
- The current plain-text SENDMSG subset rejects NUL-separated attachment/extended-message data.
- File transfer, encryption, identity extensions, and other unimplemented semantics are not inferred or executed.

A future extension may be safely ignored only when its effect on the supported base operation is specified to be irrelevant and that behavior has explicit tests. Unknownness alone is not evidence that ignoring is safe.

## Consequences

### Positive

- Syntax errors, unsupported features, and policy rejections are distinguishable.
- Raw option values remain available for diagnostics and future implementation.
- Unsupported protocol features fail closed without being mislabeled as malformed packets.
- Typed events form a narrow boundary for state transitions and formal proofs.
- New extensions can be added with local parsers and tests without complicating the common envelope parser.

### Trade-offs

- Some packets from newer or more feature-rich clients will be parsed but not acted upon.
- The current classifier rejects all unimplemented option bits, including bits that may be harmless for the supported base operation. This is intentionally conservative until command-specific compatibility behavior is established.
- The parser's correctness does not establish semantic compatibility or sender authenticity.

## Verification and testing

Tests must cover:
- Raw command-word and option-bit preservation at the parser boundary.
- Unknown command IDs remaining unsupported.
- Unsupported option bits producing `UnsupportedOptions` with the exact command ID and flag value.
- Supported command/option combinations producing the expected typed event.
- Unsupported extensions producing no state transition.
- Malformed envelopes remaining parse errors.

Verus proof claims remain limited to the designated protocol-core functions and their documented assumptions. This ADR does not claim end-to-end protocol correctness or whole-application verification.

## References

- [MVP protocol subset and extension policy](../protocol.md)
- [Security model](../security-model.md)
- [Verification strategy](../verification.md)
- Upstream IP Messenger protocol draft 14: https://github.com/shirouzu/ipmsg/blob/master/protocol.txt
