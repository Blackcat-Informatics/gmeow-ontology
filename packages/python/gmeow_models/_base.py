"GMEOW Pydantic v2 models: one model per compiled SHACL shape, with SHACL-derived Field constraints and a class-owned model_json_schema() hook onto the shared compiled $defs."
from __future__ import annotations

from pydantic import BaseModel, ConfigDict


class PurrdfBaseModel(BaseModel):
    model_config = ConfigDict(
        populate_by_name=False,
    )
