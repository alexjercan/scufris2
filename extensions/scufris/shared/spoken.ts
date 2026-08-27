/**
 * Event carrying what Scufris said, from whoever knows it to whoever sends it.
 *
 * Scufris answers through a `scufris_final_response` tool call rather than an
 * assistant text block, so nothing outside the extension that owns that tool
 * can read the answer off the conversation. It says so here, and the service
 * extension is what carries it to the transcript and to the speaker.
 *
 * Two fields because they are two different strings and two different
 * decisions. The transcript holds the whole answer; speech holds the paragraph
 * shaped for a speaker, and only when the speech mode says there is one worth
 * saying.
 */
export const SPOKEN_EVENT = "scufris:spoken";

/** What [`SPOKEN_EVENT`] carries. Either half may be absent. */
export interface SpokenSignal {
  /** The whole answer, for the transcript. */
  said?: string;
  /** The paragraph to synthesise, when the speech mode asks for one. */
  speak?: string;
}
