import { NavLink, Route, Routes } from "react-router-dom";
import { SubmitPage } from "./pages/SubmitPage";
import { LeaderboardPage } from "./pages/LeaderboardPage";

export function App() {
  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <span className="logo">SubFixer</span>
          <span className="tagline">crowd-sourced caption corrections</span>
        </div>
        <nav>
          <NavLink to="/" end>
            Submit
          </NavLink>
          <NavLink to="/leaderboard">Leaderboard</NavLink>
        </nav>
      </header>
      <main>
        <Routes>
          <Route path="/" element={<SubmitPage />} />
          <Route path="/leaderboard" element={<LeaderboardPage />} />
        </Routes>
      </main>
      <footer>
        <small>
          SubFixer — fix a caption, climb the leaderboard. Diffs are stored
          against the original auto-generated text, keyed by video and timestamp.
        </small>
      </footer>
    </div>
  );
}
