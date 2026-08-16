enum ExternalReturnPhase { normal, externalSheet, returningToSource }

class ExternalReturnStateMachine {
  var _phase = ExternalReturnPhase.normal;
  var _flowId = 0;
  var _wasPaused = false;

  ExternalReturnPhase get phase => _phase;
  bool get shouldHideMainUi => _phase != ExternalReturnPhase.normal;

  int beginExternalSheet() {
    _flowId += 1;
    _phase = ExternalReturnPhase.externalSheet;
    return _flowId;
  }

  bool beginReturn(int flowId) {
    if (flowId != _flowId || _phase != ExternalReturnPhase.externalSheet) {
      return false;
    }
    _phase = ExternalReturnPhase.returningToSource;
    _wasPaused = false;
    return true;
  }

  bool returnFailed(int flowId) {
    if (flowId != _flowId || _phase != ExternalReturnPhase.returningToSource) {
      return false;
    }
    _phase = ExternalReturnPhase.normal;
    return true;
  }

  bool onPaused() {
    if (_phase != ExternalReturnPhase.returningToSource) return false;
    _wasPaused = true;
    return true;
  }

  bool onResumed() {
    if (_phase != ExternalReturnPhase.returningToSource || !_wasPaused) {
      return false;
    }
    _phase = ExternalReturnPhase.normal;
    _wasPaused = false;
    return true;
  }
}
