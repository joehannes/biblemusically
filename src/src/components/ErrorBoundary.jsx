import { Component } from "react";
import { reportError } from "../lib/errorReporting";

// A crash that takes the whole interface to a white screen tells the user nothing and tells the
// author nothing. This catches it, says what happened in a sentence, keeps the work that is already
// on disk, and files the report on its own — because somebody staring at a broken screen should not
// also have to describe it before anything happens.
//
// Reloading is offered rather than performed. This app autosaves into a git repo, so a reload is
// almost always safe — but "almost always" is not a decision to make on somebody else's behalf.

export default class ErrorBoundary extends Component {
  constructor(props) {
    super(props);
    this.state = { error: null };
  }

  static getDerivedStateFromError(error) {
    return { error };
  }

  componentDidCatch(error, info) {
    reportError(error, { where: this.props.where || "interface" });
    // The component stack says which panel died, which the JS stack often does not.
    if (info?.componentStack) {
      reportError(new Error(`${error?.message || "render"} — in ${firstComponent(info.componentStack)}`),
                  { where: "render", silent: true });
    }
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="p-8 max-w-xl mx-auto space-y-3">
        <h1 className="text-xl font-semibold">This screen stopped working</h1>
        <p className="text-sm text-muted-foreground">
          The rest of the app is fine, and nothing you have made is lost — everything is saved to
          your project folder as you go. The fault has been reported already; you do not need to
          describe it.
        </p>
        <pre className="text-[11px] bg-muted/40 rounded-lg p-3 overflow-x-auto" data-no-i18n>
          {String(this.state.error?.message || this.state.error)}
        </pre>
        <div className="flex gap-2">
          <button className="rounded-lg bg-primary text-primary-foreground px-3 py-1.5 text-sm"
                  onClick={() => this.setState({ error: null })}>
            Try this screen again
          </button>
          <button className="rounded-lg border border-border px-3 py-1.5 text-sm"
                  onClick={() => window.location.reload()}>
            Reload the app
          </button>
        </div>
      </div>
    );
  }
}

/** The first real component name in a React component stack. */
function firstComponent(stack) {
  const line = String(stack).split("\n").find((l) => l.trim().startsWith("at "));
  return line ? line.trim().replace(/^at\s+/, "").split(" ")[0] : "an unknown component";
}
