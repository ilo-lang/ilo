// build.rs — ai.txt is a hand-maintained bootstrap index (ILO-538), no longer
// regenerated from SPEC.md.
//
// History: this script used to compact SPEC.md into ai.txt on every build,
// which kept the two in lockstep but let ai.txt balloon to ~48K tokens as the
// spec grew — the agentlanguages.dev review's "the spec has grown against the
// thesis". ILO-538 replaced ai.txt with a <3K-token index (language kernel +
// `ilo skill get <module>` load instructions); the modular skill files under
// skills/ilo/ are the content artefacts, each under a CI-enforced token cap.
//
// The regeneration had to be REMOVED, not just skipped: any local build
// otherwise overwrote the index with the SPEC-derived monolith (and CI's
// `git diff --exit-code ai.txt` then failed on the very commit that shipped
// the index). SPEC.md remains the human/reference document; `ilo -ai` embeds
// ai.txt verbatim via include_str!.

fn main() {
    // Rebuild when the index changes so the include_str! embed stays current.
    println!("cargo:rerun-if-changed=ai.txt");
}
