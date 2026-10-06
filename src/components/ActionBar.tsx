interface Props {
  editing?: boolean;
  plugin?: boolean;
  onActions?: () => void;
  menuOpen?: boolean;
}

/** 安静的品牌底栏；插件的次要管理操作集中在动作入口。 */
export function ActionBar({
  editing = false,
  plugin = false,
  onActions,
  menuOpen,
}: Props) {
  return (
    <footer className="action-bar" data-testid="action-bar">
      <span className="product-signature">
        <svg
          viewBox="42 25 46 78"
          width="12"
          height="15"
          fill="currentColor"
          aria-hidden="true"
        >
          <path d="M77 25 42 59h23Z M88 65H66l-11 38Z" />
        </svg>{" "}
        FLASHCAST
      </span>
      {editing ? (
        <span className="action-hint">
          <kbd>Ctrl</kbd>
          <kbd>Enter</kbd> 保存
        </span>
      ) : null}
      {plugin && !editing ? (
        <button
          className="action-toggle"
          aria-expanded={menuOpen}
          data-testid="actions-toggle"
          onClick={onActions}
        >
          动作 <kbd>Ctrl K</kbd>
        </button>
      ) : null}
    </footer>
  );
}
