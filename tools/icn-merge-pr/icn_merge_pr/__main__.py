"""Entry point. Also the place the import path is pinned.

The generated launcher runs this file with `python3 -E -s`, so PYTHONPATH and the per-user site
directory cannot contribute modules. Running a FILE puts that file's directory at `sys.path[0]`,
not the working directory, so a checkout the operator happens to be standing in is not on the
path either. Replacing `sys.path[0]` with the install root leaves the installed package first in
every case, which is the property that matters: the code that decides a merge is the code that
was installed from a proved-trusted commit, never code shipped by the change under evaluation.
"""

import os
import sys

_LIB = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path[0] = _LIB

from icn_merge_pr.cli import main  # noqa: E402  (path must be pinned before the import)

if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
