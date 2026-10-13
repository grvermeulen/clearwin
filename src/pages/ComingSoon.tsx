export default function ComingSoon({ title, text }: { title: string; text: string }) {
  return (
    <div className="page">
      <h1>{title}</h1>
      <div className="empty">
        <p>{text}</p>
        <p className="muted">Coming in a later version of ClearWin.</p>
      </div>
    </div>
  );
}
