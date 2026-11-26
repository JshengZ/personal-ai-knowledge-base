import { guideSteps } from "../guide";
import { Modal } from "../components/ui";

export function Welcome({
  onStart,
  onSkip,
}: {
  onStart: () => void;
  onSkip: () => void;
}) {
  return (
    <Modal
      title="开始使用个人知识库"
      onClose={onSkip}
      onSubmit={(event) => {
        event.preventDefault();
        onStart();
      }}
      submitLabel="开始导览"
      cancelLabel="跳过导览"
      className="welcome-dialog"
      focusHeading
    >
      <p>{guideSteps.length} 个步骤，了解资料索引、模型设置和 MCP 连接。</p>
      <p className="muted">
        页面会自动切换，点击“下一步”浏览即可。实际配置可稍后完成。
      </p>
      <p className="muted">
        随时可以跳过，之后从侧栏或设置中的“使用指南”重新查看。
      </p>
    </Modal>
  );
}
