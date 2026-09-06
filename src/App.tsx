import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";

function App() {
  const { data, isLoading, error } = useQuery({
    queryKey: ["ping"],
    queryFn: () => invoke<string>("ping"),
  });

  return (
    <main className="flex min-h-screen flex-col items-center justify-center gap-4">
      <h1 className="font-display text-7xl italic text-vellum">Crate</h1>
      <p className="font-mono text-sm text-slate">
        {isLoading && "contacting rust…"}
        {error && `error: ${String(error)}`}
        {data && `rust says: ${data}`}
      </p>
    </main>
  );
}

export default App;
