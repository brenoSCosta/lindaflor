import { getRouteApi, Link, useNavigate } from "@tanstack/react-router";
import { useTimeout } from "usehooks-ts";

const REDIRECT_DELAY_MS = 1500;

const route = getRouteApi("/verify-email");

export function VerifyEmail() {
  const navigate = useNavigate();
  const { error } = route.useSearch();

  useTimeout(
    () => {
      void navigate({ to: "/dashboard" });
    },
    error ? null : REDIRECT_DELAY_MS,
  );

  if (error) {
    return (
      <div className="mx-auto w-full mt-10 max-w-md p-6 text-center">
        <h1 className="mb-2 text-3xl font-bold">Falha na verificação</h1>
        <p className="mb-6 text-sm text-muted-foreground">
          O link pode estar expirado ou já foi usado. Solicite um novo na página
          de login.
        </p>
        <Link
          to="/login"
          className="text-sm text-indigo-600 hover:text-indigo-800"
        >
          Voltar para o login
        </Link>
      </div>
    );
  }

  return (
    <div className="mx-auto w-full mt-10 max-w-md p-6 text-center">
      <h1 className="mb-2 text-3xl font-bold">E-mail verificado</h1>
      <p className="text-sm text-muted-foreground">
        Redirecionando você agora…
      </p>
    </div>
  );
}
