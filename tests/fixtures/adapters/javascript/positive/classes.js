// Public class export — adapter should flag kind "class".
export class UserService {
  constructor(db) {
    this.db = db;
  }

  find(id) {
    return this.db.get(id);
  }
}
