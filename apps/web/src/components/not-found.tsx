import { ArrowLeft } from "lucide-react";
import { LazyMotion, domAnimation, m } from "motion/react";

export function NotFound() {
  return (
    <div className="relative min-h-screen w-full overflow-hidden bg-gray-950 font-sans selection:bg-lime-500/30">
      <div className="relative z-10 flex min-h-screen flex-col items-center justify-center p-4">
        <div className="relative z-10 max-w-2xl space-y-8 text-center">
          <LazyMotion features={domAnimation}>
            <m.div
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.5, delay: 0.2 }}
              className="relative inline-block"
            >
              <h1
                className="text-8xl font-black tracking-tighter text-transparent select-none md:text-[10rem]"
                style={{ WebkitTextStroke: "2px rgba(132, 204, 22, 0.2)" }}
              >
                404
              </h1>
              <m.h1
                animate={{ x: [-3, 3, -3], opacity: [0.8, 1, 0.8] }}
                transition={{
                  duration: 0.15,
                  repeat: Infinity,
                  repeatType: "mirror",
                }}
                className="absolute inset-0 text-8xl font-black tracking-tighter text-lime-500 mix-blend-screen select-none md:text-[10rem]"
                style={{ clipPath: "polygon(0 0, 100% 0, 100% 45%, 0 45%)" }}
              >
                404
              </m.h1>
              <m.h1
                animate={{ x: [3, -3, 3], opacity: [0.8, 1, 0.8] }}
                transition={{
                  duration: 0.25,
                  repeat: Infinity,
                  repeatType: "mirror",
                }}
                className="absolute inset-0 text-8xl font-black tracking-tighter text-lime-400 mix-blend-screen select-none md:text-[10rem]"
                style={{
                  clipPath: "polygon(0 55%, 100% 55%, 100% 100%, 0 100%)",
                }}
              >
                404
              </m.h1>
              <h1 className="absolute inset-0 text-8xl font-black tracking-tighter text-foreground select-none md:text-[10rem]">
                404
              </h1>
            </m.div>

            <m.div
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={{ duration: 0.5, delay: 0.4 }}
              className="border border-lime-500/10 bg-card/60 p-6 backdrop-blur-sm"
            >
              <div className="mb-4 flex items-center justify-center gap-3">
                <h2 className="text-xl font-bold tracking-[0.2em] text-lime-500 uppercase md:text-2xl">
                  Página Não Encontrada
                </h2>
              </div>

              <p className="leading-relaxed text-muted-foreground md:text-lg">
                O link que você acessou pode estar quebrado, a página ter sido
                removida ou o endereço não existir.
              </p>
            </m.div>

            <m.div
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.5, delay: 0.6 }}
              className="flex flex-col items-center justify-center gap-4 pt-4 sm:flex-row"
            >
              <a
                href="/"
                className="group relative overflow-hidden bg-lime-500 px-8 py-4 text-sm font-bold tracking-widest text-primary-foreground uppercase transition-all duration-300 hover:scale-105"
              >
                <div className="absolute inset-0 translate-y-full bg-background transition-transform duration-300 ease-in-out group-hover:translate-y-0" />
                <div className="relative flex items-center gap-2">
                  <ArrowLeft className="size-4 transition-transform group-hover:-translate-x-1" />
                  <span>Voltar ao Início</span>
                </div>
              </a>

              <button
                type="button"
                onClick={() => window.location.reload()}
                className="border border-lime-500/30 px-8 py-4 text-sm font-bold tracking-widest text-primary uppercase transition-all duration-300 hover:bg-primary hover:text-primary-foreground"
              >
                Tentar Novamente
              </button>
            </m.div>
          </LazyMotion>
        </div>
      </div>
    </div>
  );
}
