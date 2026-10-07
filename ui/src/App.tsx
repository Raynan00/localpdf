import { Home } from "./Home";
import { Job } from "./Job";

export function App() {
  const route = window.location.hash.replace(/^#/, "");
  const job = route.match(/^\/job\/(\d+)$/);
  if (job) return <Job id={Number(job[1])} />;
  return <Home />;
}
