# Rev
Rev is a desktop app for code review and revisions with coding agents.

For any bug, idea, or feedback, please [open a GitHub issue](https://github.com/peterdpong/rev/issues/new).

> [!NOTE]
> Rev is in active development and currently not stable.

With Rev you should be able to
- Review code changes.
- Ask questions to local coding agents and gain understanding about the code change.
- Propose revisions to the code change and send them to be applied by local coding agents.

Run `npm start`, then choose **Open Folder…** (or press **⌘O**) to load a local Git repository. Rev starts with an empty window until you open a folder.

You can also open a repository directly with `npm start -- --repo /path/to/repo`. Add `--base main` to choose the comparison base, or `--working` to review uncommitted changes.

