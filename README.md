# delta_cmd

A new better name is probably coming but this is very WIP so count that as a pending task.

The idea of this project is that when you're running things like `cargo test` or other tools
in CI where you can add a `cargo <CMD> --exclude` or `cargo <CMD> --include` to include/exclude
packages what if you automatically only ran the command on the packages impacted by a change.

That might save some CI time in workspace projects with a bunch of members. 

This (currently) works by:

1. Getting the files changed in the last git commit
2. Getting package metadata from cargo_metadata and dependencies in the workspace into a trie keyed on the path
3. Using the files to find an ancestor in the trie 
4. Looking at other packages in the workspace for ones that depend on the changed package
5. Boom we have all the packages impacted by the commit change - generate a command based on that
