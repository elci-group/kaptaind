namespace :db do
  task migrate: :environment do
    puts "migrating"
  end
end
