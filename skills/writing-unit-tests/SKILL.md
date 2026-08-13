---
description: >
  Load when writing, reviewing, or judging unit tests. Covers systematic
  design, boundaries, real vs. filler assertions, what to mock, determinism, 
  and project test conventions.
name: writing-unit-tests
---

# Writing Unit Tests

Aim for a suite that is correct (passes every legal implementation), thorough (fails a buggy one), and small (fast to write, run, update). Your goal as a tester is to make the code fail, not watch it pass.

## Real behavior, not filler

- Every test needs an assertion that would actually fail if the behavior broke. Code run with no assertion on its outcome is not a test.
- Asserting throw/no-throw is real when that's the function's contract (e.g. a validator that throws on invalid input). It's filler when used as a weak stand-in for a return value or side effect that exists and isn't checked.
- Don't test mocked behavior instead of real logic. Verifying a mock returned what you told it to proves nothing about the code under test.

## Design cases by partitioning

- Divide the legal input space into disjoint, complete, nonempty subdomains where behavior differs; pick one case per subdomain.
- For multiple parameters, prefer separate partitions per parameter over the Cartesian product; add an extra partition for interactions that cause bugs (e.g. sign of `a*b`).
- Partitions may be expressed on the output when behavior variation is more visible there.
- Document the strategy in a comment atop the test group; name each test by the subdomain(s) it covers.

## Always include boundaries

Bugs cluster at boundaries. Add single-element subdomains so each is always tested:
- 0, and the min/max of numeric types (overflow/precision limits)
- empty collections: empty string, array, set, map
- first and last element of a sequence
- the identity value for the operation (e.g. 1 for multiply)

## Cover success and failure paths

- Not just the happy path. Include edge cases and invalid input wherever the code branches for them.
- Thoroughly test error handling. Prefer many small tests over few large ones.
- Test both paths on all branches, including subtle ones like ternary statements.

## Mocking

- Mock external dependencies: network, filesystem, hardware, other services.
- Don't mock the class under test's own internals — that tests the mock, not the code.

## Determinism

- No flaky tests from timing (`sleep`), unseeded randomness, or uncontrolled concurrency.
- If using random values, always set the seed.
- Randomized testing supplements systematic testing; it never replaces it.

## Verify honestly

- If a function returns a status code, always verify it.
- Assert `cudaSuccess` for relevant CUDA API calls.
- Order assertions actual-first, expected-second (matters for failure messages).
- For structures, use deep-equality when the exact value is known, or assert on individual properties when multiple outputs are correct.

## Running tests

Use the build-and-test-summarizer agent to run tests and summarize failures — test output is verbose.
