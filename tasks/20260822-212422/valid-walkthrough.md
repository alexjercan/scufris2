# Example exact-revision walkthrough

This fixture is readable without the interactive renderer.

:::walkthrough
status: ready
revision: 2222222222222222222222222222222222222222
baseRevision: 1111111111111111111111111111111111111111
files: 1
added: 2
removed: 1
preflight: passed
:::

## Safe filtering

:::change
id: safe-filtering
importance: critical
file: src/actions.ts
lines: 42-43
:::

Code-confirmed fact: the new branch returns only validated actions.

Author-reported rationale: prevent unsupported actions from reaching callers.

Reviewer inference: keeping the check near the return reduces unsafe call paths.

Unknown reasoning: no evidence explains why this layer owns the policy.

```diff
-return actions;
+return actions.filter(isValid);
```

:::review
Verify that filtering at this layer covers every caller.
:::
