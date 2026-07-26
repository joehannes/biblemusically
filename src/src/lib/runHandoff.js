// When a run is in flight on the /workflow page and the user switches project, the page unmounts and its
// loop dies with it. That used to abandon the run half way while its already-queued jobs kept finishing,
// which looks like success and is not.
//
// So a page that is running registers its intent here, and the project switch hands that intent to the
// backend runner, which finishes the job with nothing of its state in the GUI.
//
// Module-level rather than context: it has to be readable from `selectProject`, which is not inside the
// page's tree, and it has to survive the unmount that is the whole problem.

let intent = null;

/** Called by the page while a run is in progress. */
export function setRunIntent(next) {
  intent = next && next.projectId ? { ...next } : null;
}

export function clearRunIntent() {
  intent = null;
}

export function runIntent() {
  return intent;
}

/** Is there an unfinished run belonging to this project? */
export function hasRunIntentFor(projectId) {
  return Boolean(intent && projectId && intent.projectId === projectId);
}
