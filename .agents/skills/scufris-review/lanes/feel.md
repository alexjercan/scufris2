# Lane: feel

Judge what Alex sees and does. The live desktop is his sign-off,
always: this lane narrows what he has to look at, it does not replace
him.

This lane wants a harness run. Take the display slot only after the
desktop and red team lanes release it. Reason from the code when you
cannot get a run, and say the judgement is unrendered.

## Look for

- State legibility with no text: the orb's shape and accent are the
  entire vocabulary. Two states that read the same at a glance, an
  accent that dies against the near-black panel, a speed change too
  subtle to notice.
- The entrance: the rise, the recoil, the page pop from the orb's own
  center. A re-presentation that replays it, a tween a hide does not
  cancel, motion that survives `prefers-reduced-motion` anywhere.
- The box: the caret exactly on the grid from the first frame, marks
  that ride the pop instead of freezing mid-animation, the two-ended
  fade on a long take, the hint saying what the keys actually do in
  this state.
- The timer: listening-only, small, dim - an afterthought by design.
  Flag it growing ambitions.
- Sound and silence: an earcon missing where a state change is
  invisible, or one firing where the change is already visible.
- The cost of a hand: how many actions recovery takes when something
  goes wrong, whether Escape always means the same thing, whether
  anything ever requires the mouse.

## Running

The harness and instruments are the desktop lane's (Xvfb plus i3
matched to the real desktop). Drive the real phases; read focus and
geometry from the X server; capture what a screenshot can prove and
name what it cannot - feel on a real screen with real timing stays
Alex's.
