using System;
using System.ComponentModel;
using System.Data.SQLite;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class CheckShiftAll
{
	private const int SizeX = 6;

	private string[,] Op;

	private int OpS;

	private string FNs;

	public int Checks => OpS;

	public string InfaCheck
	{
		get
		{
			if (x < 0 || x > 6)
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

	internal TypChecks InfaChecks(int i)
	{
		TypChecks result = default(TypChecks);
		result.IdCheck = "";
		result.NumberCheck = "";
		result.SumCheck = "";
		result.TypCheck = "";
		result.DateCheck = "";
		if (i < 1)
		{
			return result;
		}
		if (i > OpS)
		{
			return result;
		}
		result.IdCheck = Op[5, i];
		result.NumberCheck = Op[1, i];
		result.DateCheck = Op[3, i];
		result.SumCheck = Op[4, i];
		result.TypCheck = Op[6, i];
		return result;
	}

	public CheckShiftAll(int sh, string Fn = "")
	{
		Op = new string[7, 1];
		FNs = Fn;
		OpS = 0;
		Op = new string[7, checked(OpS + 1)];
		LoadSheftsAll(sh);
	}

	private void LoadSheftsAll(int ch)
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
				sQLiteCommand.CommandText = "Select * FROM ksef WHERE shiftid ='" + ch + "'";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				while (sQLiteDataReader.Read())
				{
					OpS++;
					ref string[,] op = ref Op;
					op = (string[,])Utils.CopyArray(op, new string[7, OpS + 1]);
					Op[0, OpS] = sQLiteDataReader[0].ToString();
					Op[1, OpS] = sQLiteDataReader[4].ToString();
					Op[2, OpS] = All.CheckNumToTyp(sQLiteDataReader[6].ToString());
					Op[3, OpS] = sQLiteDataReader[10].ToString();
					Op[4, OpS] = sQLiteDataReader[7].ToString();
					Op[5, OpS] = sQLiteDataReader[11].ToString();
					Op[6, OpS] = sQLiteDataReader[6].ToString();
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				OpS = 0;
				Op = new string[7, OpS + 1];
				All.Lg.SaveTextToLog("LoadSheftsAll", "Ошибка чтения чеков из таблицы ksef");
				ProjectData.ClearProjectError();
			}
		}
	}
}
