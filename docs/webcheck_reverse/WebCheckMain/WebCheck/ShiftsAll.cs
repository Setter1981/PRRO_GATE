using System;
using System.ComponentModel;
using System.Data.SQLite;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class ShiftsAll
{
	private string[,] Op;

	private int OpS;

	private string FNs;

	public int Shifts => OpS;

	public string InfaSheft
	{
		get
		{
			if (x < 0 || x > 5)
			{
				return "";
			}
			if ((y < 1) | (y > OpS))
			{
				return "";
			}
			return Op[x, y];
		}
	}

	public ShiftsAll(string Ms = "", string Ys = "", string Fn = "")
	{
		Op = new string[6, 1];
		OpS = 0;
		Op = new string[6, checked(OpS + 1)];
		FNs = Fn;
		if ((Operators.CompareString(Ms, "", false) == 0) | (Operators.CompareString(Ys, "", false) == 0))
		{
			LoadSheftsAll();
		}
		else
		{
			LoadSheftsYM(Ms, Ys);
		}
	}

	private void LoadSheftsAll()
	{
		checked
		{
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				if (All.A.Status)
				{
					sQLiteConnection.ConnectionString = All.A.Connection;
				}
				else
				{
					string text = All.f.StringGetFn(FNs, "Path");
					sQLiteConnection.ConnectionString = "Data Source=" + text + "; Version=3";
				}
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "Select * FROM SHIFTS";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				while (sQLiteDataReader.Read())
				{
					OpS++;
					ref string[,] op = ref Op;
					op = (string[,])Utils.CopyArray((Array)op, (Array)new string[6, OpS + 1]);
					Op[0, OpS] = sQLiteDataReader[0].ToString();
					try
					{
						Op[1, OpS] = sQLiteDataReader[2].ToString();
					}
					catch (Exception ex)
					{
						ProjectData.SetProjectError(ex);
						Exception ex2 = ex;
						Op[1, OpS] = "????-??-?? ??";
						ProjectData.ClearProjectError();
					}
					try
					{
						Op[2, OpS] = sQLiteDataReader[3].ToString();
					}
					catch (Exception ex3)
					{
						ProjectData.SetProjectError(ex3);
						Exception ex4 = ex3;
						Op[2, OpS] = "відкрита";
						ProjectData.ClearProjectError();
					}
					Op[3, OpS] = sQLiteDataReader[5].ToString();
					Op[4, OpS] = sQLiteDataReader[12].ToString();
					Op[5, OpS] = sQLiteDataReader[10].ToString();
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
			}
			catch (Exception ex5)
			{
				ProjectData.SetProjectError(ex5);
				Exception ex6 = ex5;
				OpS = 0;
				Op = new string[6, OpS + 1];
				ProjectData.ClearProjectError();
			}
		}
	}

	private void LoadSheftsYM(string sM, string sY)
	{
		checked
		{
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				if (All.A.Status)
				{
					sQLiteConnection.ConnectionString = All.A.Connection;
				}
				else
				{
					string text = All.f.StringGetFn(FNs, "Path");
					sQLiteConnection.ConnectionString = "Data Source=" + text + "; Version=3";
				}
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "Select * FROM SHIFTS WHERE  strftime('%m',SHIFTS.DATEBEG) ='" + sM + "' AND strftime('%Y',SHIFTS.DATEBEG) ='" + sY + "'";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				while (sQLiteDataReader.Read())
				{
					OpS++;
					ref string[,] op = ref Op;
					op = (string[,])Utils.CopyArray((Array)op, (Array)new string[6, OpS + 1]);
					Op[0, OpS] = sQLiteDataReader[0].ToString();
					try
					{
						Op[1, OpS] = sQLiteDataReader[2].ToString();
					}
					catch (Exception ex)
					{
						ProjectData.SetProjectError(ex);
						Exception ex2 = ex;
						Op[1, OpS] = "????-??-?? ??";
						ProjectData.ClearProjectError();
					}
					try
					{
						Op[2, OpS] = sQLiteDataReader[3].ToString();
					}
					catch (Exception ex3)
					{
						ProjectData.SetProjectError(ex3);
						Exception ex4 = ex3;
						Op[2, OpS] = "відкрита";
						ProjectData.ClearProjectError();
					}
					Op[3, OpS] = sQLiteDataReader[5].ToString();
					Op[4, OpS] = sQLiteDataReader[12].ToString();
					Op[5, OpS] = sQLiteDataReader[10].ToString();
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
			}
			catch (Exception ex5)
			{
				ProjectData.SetProjectError(ex5);
				Exception ex6 = ex5;
				OpS = 0;
				Op = new string[6, OpS + 1];
				ProjectData.ClearProjectError();
			}
		}
	}
}
