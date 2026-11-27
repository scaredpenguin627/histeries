interface NativeBoard {
  endTurn(): boolean;
}

@Entry
@Component
struct Board {
  private board?: NativeBoard;
  @State ready: boolean = false;
  @State turnFailed: boolean = false;

  build() {
    Column() {
      XComponent({ id: 'board', type: XComponentType.SURFACE, libraryname: 'histeries_board' })
        .width('100%')
        .layoutWeight(1)
        .onLoad((context?: object) => {
          this.board = context as NativeBoard;
          this.ready = context !== undefined;
          this.turnFailed = false;
        })
        .onDestroy(() => {
          this.ready = false;
          this.board = undefined;
        })
      Row({ space: 12 }) {
        if (this.turnFailed) {
          Text($r('app.string.turn_failed'))
        }
        Button($r('app.string.end_turn'))
          .enabled(this.ready)
          .onClick(() => {
            this.turnFailed = this.board?.endTurn() !== true;
          })
      }
      .width('100%')
      .padding(8)
      .justifyContent(FlexAlign.End)
    }
    .width('100%')
    .height('100%')
  }
}
