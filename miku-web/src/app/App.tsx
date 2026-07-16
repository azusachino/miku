import { Navigate, Route, Routes } from "react-router-dom";
import { WorkspaceScreen } from "../features/workspace/WorkspaceApp";

export function App() {
  return (
    <Routes>
      <Route path="/p/*" element={<WorkspaceScreen />} />
      <Route path="/folder/*" element={<WorkspaceScreen />} />
      <Route path="/tags/*" element={<WorkspaceScreen />} />
      <Route path="/recent" element={<WorkspaceScreen />} />
      <Route path="/settings" element={<Navigate replace to="/" />} />
      <Route path="*" element={<WorkspaceScreen />} />
    </Routes>
  );
}
