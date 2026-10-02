# Raw provider output

`generation_text` carries provider-visible content, optional reasoning fields,
and structured tool-name/argument fragments before reply cleanup or novelty
tokenization. It preserves whitespace and Unicode; it does not require logprobs.
The feed retains 16 generations with at most 65,536 characters each, including
completed, failed and interleaved background generations. Completion and chat
preview clearing do not erase it. The operator can clear it explicitly.

An API delta is not necessarily one tokenizer token. Hidden activations and
upstream template/control tokens not exposed by the provider are not synthesized.
Non-streaming fallback responses appear on completion. Regular text-generation
background calls now request streaming when observed, with a bounded fallback
only for an explicit unsupported-stream HTTP response.
