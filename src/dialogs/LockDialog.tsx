import { Dialog } from "../components/Dialog";
import type { AppCtx } from "../app-context";

export function LockDialog({ ctx, onClose }: { ctx: AppCtx; onClose: () => void }) {
  return (
    <Dialog title="锁定" onClose={onClose}>
      <div className="muted" style={{ fontSize: 14, lineHeight: 1.7 }}>
        立即锁定密码库并返回解锁页面。未保存的编辑会丢失，已保存的内容受主密码保护。
      </div>
      <div className="spacer-16" />
      <button
        className="btn danger block tall"
        onClick={() => {
          onClose();
          ctx.lockNow();
        }}
      >
        锁定密码库
      </button>
    </Dialog>
  );
}
