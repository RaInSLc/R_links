import { AppErrorBoundary } from "./AppErrorBoundary";
import { AppContent } from "./AppContent";

function App() {
  return (
    <AppErrorBoundary>
      <AppContent />
    </AppErrorBoundary>
  );
}

export default App;
