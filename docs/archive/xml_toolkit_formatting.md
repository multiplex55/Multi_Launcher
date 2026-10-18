# XML input and formatting policy

XML supplied to the engine is already a Rust UTF-8 string. An XML declaration
may omit `encoding` or name UTF-8; other declared encodings are rejected with a
parse diagnostic instead of being interpreted against the already-decoded
string. The secure MSXML settings established by M7-A remain in force.

An empty editor source is an `Empty` session state: it clears transformed
output and closes the prior document on the XML worker without asking MSXML to
parse an empty string. Parsing creates the authoritative COM document once
for each nonempty accepted source revision. Format and Minify operate on a
deep clone of that document. Their result is separate output; a caller must
explicitly replace editor source with that result. Formatting does not
invalidate XPath or schema work against the source revision.

Pretty output uses two-space indentation and inserts line breaks only inside
elements that contain no text or CDATA child nodes, are outside inherited
`xml:space="preserve"`, and are not descendants of mixed-content elements.
The mixed-content protection covers the whole nested subtree so indentation
cannot change a parent's XPath string value. An explicit nested
`xml:space="default"` resumes the ordinary whitespace policy where mixed
content does not also protect that subtree. Existing text nodes, including
whitespace-only nodes, are retained exactly. This conservative rule avoids
guessing whether whitespace is significant when no schema supplies that
information. Minify
serializes without adding indentation and never trims text or whitespace; a
source that already contains indentation can therefore retain it. XML
declarations, namespace-qualified names, comments, processing instructions,
CDATA sections, attribute values, and Unicode remain represented in the
serialized tree, though MSXML may normalize lexical details such as entity
spelling, empty-element spacing, or quote style.

XML text input is limited to 4 MiB. Formatting rejects documents over 256 nested
levels, 200,000 visited nodes, 4 MiB of inserted indentation, or 16 MiB of
serialized output. XPath node snapshots are limited to 10,000 nodes and 4 MiB
of aggregate copied node text; this also bounds duplicated descendant text
from selecting nested elements.

The session mailbox retains at most one pending source and one pending format
request. UI callers can request shutdown without blocking, retain the session
owner, and poll `try_reap` until the worker is terminal; application shutdown
can use the synchronous `shutdown` path away from egui.
