defmodule MyAppWeb.Router do
  use MyAppWeb, :router

  pipeline :api do
    plug :accepts, ["json"]
  end

  scope "/api", MyAppWeb do
    get "/users", UserController, :index
    post "/users", UserController, :create
    resources "/posts", PostController, except: [:delete]
  end
end
