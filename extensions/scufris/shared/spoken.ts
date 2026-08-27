/**
 * Event carrying what Scufris said, from whoever knows it to whoever sends it.
 *
 * Scufris answers through a `scufris_final_response` tool call rather than an
 * assistant text block, so nothing outside the extension that owns that tool
 * can read the answer off the conversation. It says so here, and the service
 * extension is what carries it to the transcript and to the speaker.
 *
 * Two fields because they have two destinations, not because anything here
 * decides between them. `said` goes in the transcript and `speak` goes to the
 * speaker, and the response extension fills both from the same paragraph.
 *
 * Whether a sound is made is not decided here and is not on this event. The
 * companion owns the speaker, so it owns the mute; a deployment with no
 * synthesiser is silent without being told.
 */
export const SPOKEN_EVENT = "scufris:spoken";

/** What [`SPOKEN_EVENT`] carries. Either half may be absent. */
export interface SpokenSignal {
  /** The whole answer, for the transcript. */
  said?: string;
  /** The paragraph to synthesise, when the speech mode asks for one. */
  speak?: string;
}
