from pathlib import Path

import yaml


CONFIG_PATH = Path(
    "config/searches.yaml"
)


def load_search_config():
    with CONFIG_PATH.open(
        "r",
        encoding="utf-8",
    ) as file:

        return yaml.safe_load(file)