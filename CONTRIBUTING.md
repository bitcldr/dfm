Feel free to open a discussion, submit issues, fork the repository, and send pull requests.

- **Create an issue first**: Before starting any work, open an issue to discuss the proposal. This avoids duplicated effort and lets us agree on the approach upfront.

- **Fork and branch**: Fork the repo and create a branch with a descriptive name tied to the issue.

- **Write tests**: Add tests for new features and bug fixes. All tests must pass (`cargo test`).

- **Pass the linter**: Run `cargo fmt --all --check` and `cargo clippy --all-targets --all-features -- -D warnings` before pushing. The CI gate is non-negotiable.

- **Keep PRs focused**: One issue per pull request. Reference the related issue in the PR description.

- **Be patient**: Maintainers may not review immediately — please be respectful while waiting for feedback.
