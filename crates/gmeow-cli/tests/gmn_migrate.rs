// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Migration command dispatch over independent user inputs. The complete authored
//! migration, registry, precedence and preservation verdicts are graded from
//! authenticated producer observations by the pipeline corpus suite.

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;

const CORRESPONDENCE: &str = "https://example.org/cli/crossing";

/// Own every tiny user input until the child command has exited.
struct Inputs(tempfile::TempDir);
impl Inputs {
    fn new(document: &str, keep_star: bool) -> Self {
        let inputs = Self(tempfile::tempdir().expect("temporary user directory"));
        fs::write(inputs.path("stored.gmn"), document).unwrap();
        fs::write(
            inputs.path("language.ttl"),
            r#"
@prefix g: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex: <https://example.org/cli/> .
g:gmnCodebookCurrent a g:GmnCodebook ; g:references ex:dictionary, ex:script ;
  g:gmnDictionaryVersion "3" ; g:gmnGlyphTableVersion "2" .
ex:dictionary a g:GmnDictionary ; g:gmnDictionaryVersion "3" .
ex:script a lang:Script ; lang:hasGrapheme ex:starGrapheme .
ex:starGrapheme g:gmnCodepoints "U+2605" .
ex:denotation a lang:Denotation ; lang:denotationTarget ex:star ;
  lang:denotedForm ex:starForm ; g:gmnDenotationGrapheme ex:starGrapheme .
ex:candidate a g:GmnSymbolCandidate ; g:gmnCandidateDenotation ex:denotation ;
  g:gmnSymbolDisposition g:gmnDispositionAdoptedGlyph ; g:gmnAsciiFallback "cliStar" .
g:gmnDialectVersions a g:VersionSet ; g:gmnAcceptWindow 1 .
ex:latest logic:versionInfo "1" .
ex:membership a g:VersionMembership ; g:versionMember ex:latest ;
  g:versionSet g:gmnDialectVersions ; g:versionRole g:roleLatest .
"#,
        )
        .unwrap();
        let mut migration = String::from(
            r#"
@prefix g: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex: <https://example.org/cli/> .
ex:source logic:versionInfo "1" .
ex:target logic:versionInfo "2" .
ex:crossing a logic:Correspondence ; g:gmnMigratesFrom ex:source ;
  g:gmnMigratesTo ex:target ; logic:preservationKind logic:ExactPreservation ;
  g:gmnMigrationRewrite ex:rewrite .
ex:rewrite g:gmnRewriteTerm ex:diamond ;
  g:gmnRewriteFromGlyph "◆" ; g:gmnRewriteToGlyph "◇" .
"#,
        );
        if keep_star {
            migration.push_str("ex:target g:gmnVersionDefinesOperator ex:star .\n");
        }
        fs::write(inputs.path("migration.ttl"), migration).unwrap();
        inputs
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.path().join(name)
    }

    fn command(&self) -> Command {
        let mut command = Command::cargo_bin("gmeow").expect("gmeow binary builds");
        command
            .args(["gmn", "migrate"])
            .arg(self.path("stored.gmn"))
            .args(["--correspondence", CORRESPONDENCE, "--migrations"])
            .arg(self.path("migration.ttl"))
            .arg("--lang-module")
            .arg(self.path("language.ttl"));
        command
    }
}

/// Dispatch emits the target header, a renamed glyph and a surviving glyph,
/// and reports the preservation judgment and operator count on stderr.
#[test]
fn gmn_migrate_reemits_stored_document_at_target_major() {
    let inputs = Inputs::new(
        "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n\
         @c{s:ex__a,p:ex__rel,o:◆}\n@c{s:ex__b,p:ex__rel,o:★}\n",
        true,
    );
    inputs
        .command()
        .assert()
        .success()
        .stdout(predicate::str::contains("@gmn{v: 2,"))
        .stdout(predicate::str::contains("o:◇}"))
        .stdout(predicate::str::contains("o:★}"))
        .stdout(predicate::str::contains("o:◆}").not())
        .stderr(predicate::str::contains(
            "preservation logic:ExactPreservation",
        ))
        .stderr(predicate::str::contains("2 operator(s) migrated"));
}

/// An uncovered source operator fails with its named failure class and term.
#[test]
fn gmn_migrate_unbridged_drop_hard_fails_with_named_class() {
    let inputs = Inputs::new(
        "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@c{s:ex__a,p:ex__rel,o:★}\n",
        false,
    );
    inputs
        .command()
        .assert()
        .failure()
        .stderr(predicate::str::contains("lang:GmnUnbridgedGlyphDrop"))
        .stderr(predicate::str::contains("https://example.org/cli/star"));
}

/// A document outside the declared source major fails instead of guessing.
#[test]
fn gmn_migrate_out_of_window_source_major_hard_fails() {
    let inputs = Inputs::new(
        "@gmn{v: 9, aliases: dict-v3, glyphs: 2}\n@c{s:ex__a,p:ex__rel,o:◆}\n",
        true,
    );
    inputs
        .command()
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "outside the crossing's source window",
        ));
}
