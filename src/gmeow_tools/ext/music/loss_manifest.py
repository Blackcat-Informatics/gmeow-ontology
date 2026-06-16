# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Declared-loss manifests for notation projections.

Each renderer emits a manifest sidecar that records the
``NotationProjectionProfile`` it honoured and the ``ProjectionLoss``es it
incurred.  The manifest is generated data, driven by a static mapping that
mirrors the music slice ontology so the public CLI can produce it without a
source checkout.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

_GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _iri(local: str) -> str:
    return _GMEOW + local


@dataclass(frozen=True)
class NotationProfile:
    """A projection profile for a single target notation."""

    notation_system: str
    projection_function: str
    representable_parameters: tuple[str, ...]
    declared_losses: tuple[str, ...]


# Static mirror of the NotationProjectionProfile individuals in
# slices/extensions/music/module.ttl.  Kept in sync with the ontology by the
# test suite (test_music_loss_manifest.py).
_PROFILES: dict[str, NotationProfile] = {
    "musicxml": NotationProfile(
        notation_system=_iri("notationMusicXML"),
        projection_function=_iri("fnProjectToMusicXML"),
        representable_parameters=(
            _iri("musicalParameterPitch"),
            _iri("musicalParameterDuration"),
            _iri("musicalParameterOrder"),
            _iri("musicalParameterTempo"),
            _iri("musicalParameterDynamics"),
            _iri("musicalParameterInstrumentation"),
            _iri("musicalParameterPerformerCount"),
            _iri("musicalParameterTacet"),
        ),
        declared_losses=(
            _iri("lossDropsSpatialSoundContext"),
            _iri("lossDropsTimbre"),
        ),
    ),
    "mei": NotationProfile(
        notation_system=_iri("notationMEI"),
        projection_function=_iri("fnProjectToMEI"),
        representable_parameters=(
            _iri("musicalParameterPitch"),
            _iri("musicalParameterDuration"),
            _iri("musicalParameterOrder"),
            _iri("musicalParameterTempo"),
            _iri("musicalParameterDynamics"),
            _iri("musicalParameterInstrumentation"),
            _iri("musicalParameterPerformerCount"),
            _iri("musicalParameterTacet"),
        ),
        declared_losses=(
            _iri("lossDropsSpatialSoundContext"),
            _iri("lossDropsTimbre"),
            _iri("lossQuantizesPitchTo12Edo"),
        ),
    ),
    "tab": NotationProfile(
        notation_system=_iri("notationTablature"),
        projection_function=_iri("fnProjectToTablature"),
        representable_parameters=(
            _iri("musicalParameterDuration"),
            _iri("musicalParameterOrder"),
            _iri("musicalParameterDynamics"),
            _iri("musicalParameterInstrumentation"),
            _iri("musicalParameterPerformerCount"),
            _iri("musicalParameterTacet"),
        ),
        declared_losses=(
            _iri("lossDropsSpatialSoundContext"),
            _iri("lossDropsTimbre"),
            _iri("lossQuantizesPitchTo12Edo"),
            _iri("lossQuantizesTimeToRationalGrid"),
        ),
    ),
    "lilypond": NotationProfile(
        notation_system=_iri("notationLilyPond"),
        projection_function=_iri("fnProjectToLilyPond"),
        representable_parameters=(
            _iri("musicalParameterPitch"),
            _iri("musicalParameterDuration"),
            _iri("musicalParameterOrder"),
            _iri("musicalParameterTempo"),
            _iri("musicalParameterDynamics"),
            _iri("musicalParameterInstrumentation"),
            _iri("musicalParameterPerformerCount"),
            _iri("musicalParameterTacet"),
        ),
        declared_losses=(
            _iri("lossDropsSpatialSoundContext"),
            _iri("lossDropsTimbre"),
            _iri("lossQuantizesPitchTo12Edo"),
        ),
    ),
    "abc": NotationProfile(
        notation_system=_iri("notationABC"),
        projection_function=_iri("fnProjectToABC"),
        representable_parameters=(
            _iri("musicalParameterPitch"),
            _iri("musicalParameterDuration"),
            _iri("musicalParameterOrder"),
            _iri("musicalParameterTempo"),
            _iri("musicalParameterDynamics"),
            _iri("musicalParameterInstrumentation"),
            _iri("musicalParameterPerformerCount"),
            _iri("musicalParameterTacet"),
        ),
        declared_losses=(
            _iri("lossDropsSpatialSoundContext"),
            _iri("lossDropsTimbre"),
            _iri("lossQuantizesPitchTo12Edo"),
        ),
    ),
    "scl": NotationProfile(
        notation_system=_iri("notationSCL"),
        projection_function=_iri("fnProjectToScl"),
        representable_parameters=(_iri("musicalParameterPitch"),),
        declared_losses=(
            _iri("lossDropsDynamics"),
            _iri("lossDropsInstrumentation"),
            _iri("lossDropsPerformerCount"),
            _iri("lossDropsSpatialSoundContext"),
            _iri("lossDropsTacet"),
            _iri("lossDropsTimbre"),
            _iri("lossQuantizesTimeToRationalGrid"),
        ),
    ),
    "midi": NotationProfile(
        notation_system=_iri("notationMIDI"),
        projection_function=_iri("fnProjectToMIDI"),
        representable_parameters=(
            _iri("musicalParameterPitch"),
            _iri("musicalParameterDuration"),
            _iri("musicalParameterOrder"),
            _iri("musicalParameterTempo"),
            _iri("musicalParameterDynamics"),
            _iri("musicalParameterTimbre"),
            _iri("musicalParameterInstrumentation"),
            _iri("musicalParameterPerformerCount"),
        ),
        declared_losses=(
            _iri("lossDropsSpatialSoundContext"),
            _iri("lossDropsTacet"),
        ),
    ),
    "kern": NotationProfile(
        notation_system=_iri("notationKern"),
        projection_function=_iri("fnProjectToKern"),
        representable_parameters=(
            _iri("musicalParameterPitch"),
            _iri("musicalParameterDuration"),
            _iri("musicalParameterOrder"),
            _iri("musicalParameterTempo"),
            _iri("musicalParameterDynamics"),
            _iri("musicalParameterInstrumentation"),
            _iri("musicalParameterPerformerCount"),
            _iri("musicalParameterTacet"),
        ),
        declared_losses=(
            _iri("lossDropsSpatialSoundContext"),
            _iri("lossDropsTimbre"),
        ),
    ),
    "mensural": NotationProfile(
        notation_system=_iri("notationMensural"),
        projection_function=_iri("fnProjectToMensural"),
        representable_parameters=(
            _iri("musicalParameterPitch"),
            _iri("musicalParameterDuration"),
            _iri("musicalParameterOrder"),
            _iri("musicalParameterTempo"),
            _iri("musicalParameterInstrumentation"),
            _iri("musicalParameterPerformerCount"),
            _iri("musicalParameterTacet"),
        ),
        declared_losses=(
            _iri("lossDropsDynamics"),
            _iri("lossDropsSpatialSoundContext"),
            _iri("lossDropsTimbre"),
        ),
    ),
    "graphic": NotationProfile(
        notation_system=_iri("notationGraphic"),
        projection_function=_iri("fnProjectToGraphic"),
        representable_parameters=(
            _iri("musicalParameterSoundContent"),
            _iri("musicalParameterLocation"),
        ),
        declared_losses=(
            _iri("lossDropsDynamics"),
            _iri("lossDropsInstrumentation"),
            _iri("lossDropsPerformerCount"),
            _iri("lossDropsTacet"),
            _iri("lossDropsTimbre"),
            _iri("lossQuantizesPitchTo12Edo"),
            _iri("lossQuantizesTimeToRationalGrid"),
        ),
    ),
}


def list_formats() -> list[str]:
    """Return the supported projection-format names."""
    return sorted(_PROFILES.keys())


def get_profile(format_name: str) -> NotationProfile:
    """Return the projection profile for ``format_name``.

    Raises:
        ValueError: if ``format_name`` is not a supported projection format.
    """
    try:
        return _PROFILES[format_name.lower()]
    except KeyError as exc:
        raise ValueError(f"unsupported projection format: {format_name}") from exc


def _ttl_multiline_literal(value: str) -> str:
    """Escape a string for safe insertion into a Turtle triple-quoted literal."""
    escaped = value.replace("\\", "\\\\").replace('"""', '\\"""')
    return f'"""{escaped}"""'


def manifest_turtle(format_name: str, *, provenance: str | None = None) -> str:
    """Return a Turtle sidecar declaring the losses for ``format_name``."""
    profile = get_profile(format_name)
    lines = [
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .",
        "",
        "[] a gmeow:InformationObject ;",
        f"    gmeow:targetNotationSystem <{profile.notation_system}> ;",
        f"    gmeow:projectionFunction <{profile.projection_function}> ;",
    ]
    if provenance:
        lines.append(f"    gmeow:generatedBy {_ttl_multiline_literal(provenance)} ;")
    params = ",\n        ".join(f"<{p}>" for p in profile.representable_parameters)
    lines.append(f"    gmeow:representableParameter {params} ;")
    losses = ",\n        ".join(f"<{loss}>" for loss in profile.declared_losses)
    lines.append(f"    gmeow:declaredLoss {losses} .")
    return "\n".join(lines) + "\n"


def import_manifest_turtle(
    source: Path, piece_iri: str, *, provenance: str | None = None
) -> str:
    """Return a Turtle sidecar documenting the inward MusicXML projection."""
    source_uri = source.absolute().as_uri()
    label = provenance or f"gmeow music import {source.name}"
    return (
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n"
        "@prefix prov: <http://www.w3.org/ns/prov#> .\n"
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n"
        "\n"
        f"<{piece_iri}> a gmeow:MusicalExpression ;\n"
        f"    prov:wasDerivedFrom <{source_uri}> .\n"
        "\n"
        "[] a prov:Activity ;\n"
        f"    rdfs:label {_ttl_multiline_literal(label)} ;\n"
        f"    prov:used <{source_uri}> .\n"
    )


@dataclass
class ManifestDiff:
    """Result of comparing a generated manifest to an ontology profile."""

    missing_parameters: list[str] = field(default_factory=list)
    extra_parameters: list[str] = field(default_factory=list)
    missing_losses: list[str] = field(default_factory=list)
    extra_losses: list[str] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        """True when the manifest matches the profile exactly."""
        return not any(
            self.missing_parameters
            or self.extra_parameters
            or self.missing_losses
            or self.extra_losses
        )


def diff_manifest_to_ontology(
    format_name: str,
    *,
    representable: set[str],
    losses: set[str],
) -> ManifestDiff:
    """Compare a runtime manifest against the static profile mapping."""
    profile = get_profile(format_name)
    profile_params = set(profile.representable_parameters)
    profile_losses = set(profile.declared_losses)
    return ManifestDiff(
        missing_parameters=sorted(profile_params - representable),
        extra_parameters=sorted(representable - profile_params),
        missing_losses=sorted(profile_losses - losses),
        extra_losses=sorted(losses - profile_losses),
    )
