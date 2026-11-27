## Summary

## Tested

Run the [checks relevant to this change](../CONTRIBUTING.md#test-what-your-change-touches).

- [ ] The checks relevant to this change passed
- [ ] Changed or new behaviour is covered by a Lanes scenario, or the reason it cannot be is stated below
- [ ] Where the table calls for AMD and NVIDIA, they passed on both. Otherwise, say which vendor still needs to run them:

GPU, driver and backend:

Commands run and their results:

```text
```

## Checks

CI runs the Python unit tests, the shader compiler, DXBC, and XG layout tests, and a Windows typecheck of `tuxbox-d3d12`.

If this pull request changes evidence JSON, regenerate the results table and the progress map and include both files:

```sh
python tuxbox.py status > docs/RESULTS.md
python tuxbox.py status --svg docs/progress.svg
```
