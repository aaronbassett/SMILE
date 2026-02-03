"""Shared utilities for SMILE Loop agent wrappers.

This module provides common file loading utilities used by both the Student
and Mentor wrappers, including configuration loading, tutorial content loading,
and mentor notes loading.
"""

from __future__ import annotations

import json
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path

    from smile_wrappers.config import Config

__all__ = [
    "load_config_from_file",
    "load_mentor_notes_from_file",
    "load_tutorial_content",
]


def load_config_from_file(config_path: Path) -> Config:
    """Load configuration from a JSON file.

    Args:
        config_path: Path to the configuration JSON file.

    Returns:
        A validated Config object.

    Raises:
        FileNotFoundError: If the config file does not exist.
        json.JSONDecodeError: If the file is not valid JSON.
        ValidationError: If the JSON does not match the Config schema.

    Example:
        >>> from pathlib import Path
        >>> config = load_config_from_file(Path("/workspace/smile.json"))
        >>> config.llm_provider
        <LlmProvider.CLAUDE: 'claude'>
    """
    # Import here to avoid circular imports at runtime
    from smile_wrappers.config import Config  # noqa: PLC0415

    with config_path.open() as f:
        data = json.load(f)
    return Config.model_validate(data)


def load_mentor_notes_from_file(notes_path: Path) -> list[str]:
    """Load mentor notes from a JSON file.

    Args:
        notes_path: Path to the mentor notes JSON file.

    Returns:
        A list of mentor note strings. Returns an empty list if the file
        does not exist.

    Example:
        >>> from pathlib import Path
        >>> notes = load_mentor_notes_from_file(Path("/workspace/mentor_notes.json"))
        >>> len(notes)
        2
    """
    if not notes_path.exists():
        return []

    with notes_path.open() as f:
        data = json.load(f)

    if isinstance(data, list):
        return [str(note) for note in data]
    return []


def load_tutorial_content(tutorial_dir: Path) -> str:
    """Load tutorial content from the tutorial directory.

    Looks for common tutorial filenames in the specified directory.
    Searches in this order: tutorial.md, README.md, index.md, TUTORIAL.md.
    If none of these are found, returns the first markdown file found.

    Args:
        tutorial_dir: Path to the directory containing the tutorial.

    Returns:
        The tutorial content as a string.

    Raises:
        FileNotFoundError: If no tutorial file is found.

    Example:
        >>> from pathlib import Path
        >>> content = load_tutorial_content(Path("/workspace/tutorial"))
        >>> "# Introduction" in content
        True
    """
    # Common tutorial filenames to look for
    tutorial_names = [
        "tutorial.md",
        "README.md",
        "index.md",
        "TUTORIAL.md",
    ]

    for name in tutorial_names:
        tutorial_path = tutorial_dir / name
        if tutorial_path.exists():
            return tutorial_path.read_text()

    # If no common name found, look for any markdown file
    md_files = list(tutorial_dir.glob("*.md"))
    if md_files:
        return md_files[0].read_text()

    raise FileNotFoundError(
        f"No tutorial file found in {tutorial_dir}. Expected one of: {', '.join(tutorial_names)}"
    )
