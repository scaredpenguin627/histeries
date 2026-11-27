import { UIAbility } from '@kit.AbilityKit';
import { window } from '@kit.ArkUI';
import { hilog } from '@kit.PerformanceAnalysisKit';

export default class EntryAbility extends UIAbility {
  onWindowStageCreate(stage: window.WindowStage): void {
    stage.loadContent('pages/Board').catch((error: Error) => {
      hilog.error(0x0001, 'Histeries', 'Board page failed to load: %{public}s', error.message);
    });
  }
}
