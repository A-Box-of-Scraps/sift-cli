export function SessionPanel() {
  const sessionTokenPreview = "redacted";
  return <div>{sessionTokenPreview}</div>;
}

declare global {
  namespace JSX {
    interface IntrinsicElements {
      div: { children: string };
    }
  }
}
