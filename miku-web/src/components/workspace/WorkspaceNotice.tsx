type WorkspaceNoticeProps = {
  message: string | null;
  onDismiss: () => void;
};

export function WorkspaceNotice({ message, onDismiss }: WorkspaceNoticeProps) {
  if (!message) return null;
  return (
    <div className="workspace-notice" role="alert">
      <span>{message}</span>
      <button className="quiet-button" onClick={onDismiss} aria-label="Dismiss notice">×</button>
    </div>
  );
}
