using System;
using System.ComponentModel;
using System.Data.SQLite;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class Form2 : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("Oinn")]
	private TextBox _Oinn;

	[CompilerGenerated]
	[AccessedThroughProperty("ADDoperator")]
	private Button _ADDoperator;

	[CompilerGenerated]
	[AccessedThroughProperty("PathB")]
	private Button _PathB;

	[field: AccessedThroughProperty("DG")]
	internal virtual DataGridView DG
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Oname")]
	internal virtual TextBox Oname
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Opath")]
	internal virtual TextBox Opath
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Opass")]
	internal virtual TextBox Opass
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual TextBox Oinn
	{
		[CompilerGenerated]
		get
		{
			return _Oinn;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Oinn_TextChanged;
			TextBox oinn = _Oinn;
			if (oinn != null)
			{
				((Control)oinn).TextChanged -= eventHandler;
			}
			_Oinn = value;
			oinn = _Oinn;
			if (oinn != null)
			{
				((Control)oinn).TextChanged += eventHandler;
			}
		}
	}

	internal virtual Button ADDoperator
	{
		[CompilerGenerated]
		get
		{
			return _ADDoperator;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ADDoperator_Click;
			Button aDDoperator = _ADDoperator;
			if (aDDoperator != null)
			{
				((Control)aDDoperator).Click -= eventHandler;
			}
			_ADDoperator = value;
			aDDoperator = _ADDoperator;
			if (aDDoperator != null)
			{
				((Control)aDDoperator).Click += eventHandler;
			}
		}
	}

	internal virtual Button PathB
	{
		[CompilerGenerated]
		get
		{
			return _PathB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = PathB_Click;
			Button pathB = _PathB;
			if (pathB != null)
			{
				((Control)pathB).Click -= eventHandler;
			}
			_PathB = value;
			pathB = _PathB;
			if (pathB != null)
			{
				((Control)pathB).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("ID")]
	internal virtual DataGridViewTextBoxColumn ID
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("OPERATORNAME")]
	internal virtual DataGridViewTextBoxColumn OPERATORNAME
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("KEYPATH")]
	internal virtual DataGridViewTextBoxColumn KEYPATH
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("KEYPASS")]
	internal virtual DataGridViewTextBoxColumn KEYPASS
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("INN")]
	internal virtual DataGridViewTextBoxColumn INN
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public Form2()
	{
		((Form)this).Load += Form2_Load;
		InitializeComponent();
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_0053: Unknown result type (might be due to invalid IL or missing references)
		//IL_005d: Expected O, but got Unknown
		//IL_005e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0068: Expected O, but got Unknown
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		//IL_0073: Expected O, but got Unknown
		//IL_0074: Unknown result type (might be due to invalid IL or missing references)
		//IL_007e: Expected O, but got Unknown
		//IL_007f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0089: Expected O, but got Unknown
		//IL_008a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0094: Expected O, but got Unknown
		//IL_02b9: Unknown result type (might be due to invalid IL or missing references)
		//IL_02c3: Expected O, but got Unknown
		//IL_033c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0346: Expected O, but got Unknown
		//IL_03c2: Unknown result type (might be due to invalid IL or missing references)
		//IL_03cc: Expected O, but got Unknown
		//IL_0448: Unknown result type (might be due to invalid IL or missing references)
		//IL_0452: Expected O, but got Unknown
		//IL_0646: Unknown result type (might be due to invalid IL or missing references)
		//IL_0650: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(Form2));
		DG = new DataGridView();
		ID = new DataGridViewTextBoxColumn();
		OPERATORNAME = new DataGridViewTextBoxColumn();
		KEYPATH = new DataGridViewTextBoxColumn();
		KEYPASS = new DataGridViewTextBoxColumn();
		INN = new DataGridViewTextBoxColumn();
		Oname = new TextBox();
		Opath = new TextBox();
		Opass = new TextBox();
		Oinn = new TextBox();
		ADDoperator = new Button();
		PathB = new Button();
		((ISupportInitialize)DG).BeginInit();
		((Control)this).SuspendLayout();
		DG.AllowUserToAddRows = false;
		DG.AllowUserToDeleteRows = false;
		((Control)DG).Anchor = (AnchorStyles)15;
		DG.ColumnHeadersHeightSizeMode = (DataGridViewColumnHeadersHeightSizeMode)2;
		DG.Columns.AddRange((DataGridViewColumn[])(object)new DataGridViewColumn[5]
		{
			(DataGridViewColumn)ID,
			(DataGridViewColumn)OPERATORNAME,
			(DataGridViewColumn)KEYPATH,
			(DataGridViewColumn)KEYPASS,
			(DataGridViewColumn)INN
		});
		((Control)DG).Location = new Point(0, 0);
		((Control)DG).Name = "DG";
		DG.ReadOnly = true;
		((Control)DG).Size = new Size(921, 286);
		((Control)DG).TabIndex = 0;
		((DataGridViewColumn)ID).HeaderText = "ID";
		((DataGridViewColumn)ID).Name = "ID";
		((DataGridViewColumn)ID).ReadOnly = true;
		((DataGridViewColumn)ID).Width = 50;
		((DataGridViewColumn)OPERATORNAME).HeaderText = "OPERATORNAME";
		((DataGridViewColumn)OPERATORNAME).Name = "OPERATORNAME";
		((DataGridViewColumn)OPERATORNAME).ReadOnly = true;
		((DataGridViewColumn)OPERATORNAME).Width = 200;
		((DataGridViewColumn)KEYPATH).HeaderText = "KEYPATH";
		((DataGridViewColumn)KEYPATH).Name = "KEYPATH";
		((DataGridViewColumn)KEYPATH).ReadOnly = true;
		((DataGridViewColumn)KEYPATH).Width = 300;
		((DataGridViewColumn)KEYPASS).HeaderText = "KEYPASS";
		((DataGridViewColumn)KEYPASS).Name = "KEYPASS";
		((DataGridViewColumn)KEYPASS).ReadOnly = true;
		((DataGridViewColumn)KEYPASS).Width = 150;
		((DataGridViewColumn)INN).HeaderText = "INN";
		((DataGridViewColumn)INN).Name = "INN";
		((DataGridViewColumn)INN).ReadOnly = true;
		((DataGridViewColumn)INN).Width = 150;
		((Control)Oname).Anchor = (AnchorStyles)6;
		((Control)Oname).Font = new Font("Microsoft Sans Serif", 11.25f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Oname).Location = new Point(12, 303);
		((Control)Oname).Name = "Oname";
		((Control)Oname).Size = new Size(161, 24);
		((Control)Oname).TabIndex = 1;
		Oname.TextAlign = (HorizontalAlignment)2;
		((Control)Opath).Anchor = (AnchorStyles)6;
		((Control)Opath).Font = new Font("Microsoft Sans Serif", 11.25f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Opath).Location = new Point(179, 303);
		((Control)Opath).Name = "Opath";
		((Control)Opath).Size = new Size(242, 24);
		((Control)Opath).TabIndex = 2;
		Opath.TextAlign = (HorizontalAlignment)2;
		((Control)Opass).Anchor = (AnchorStyles)6;
		((Control)Opass).Font = new Font("Microsoft Sans Serif", 11.25f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Opass).Location = new Point(513, 303);
		((Control)Opass).Name = "Opass";
		((Control)Opass).Size = new Size(225, 24);
		((Control)Opass).TabIndex = 3;
		Opass.TextAlign = (HorizontalAlignment)2;
		((Control)Oinn).Anchor = (AnchorStyles)6;
		((Control)Oinn).Font = new Font("Microsoft Sans Serif", 11.25f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Oinn).Location = new Point(744, 303);
		((Control)Oinn).Name = "Oinn";
		((Control)Oinn).Size = new Size(165, 24);
		((Control)Oinn).TabIndex = 4;
		Oinn.TextAlign = (HorizontalAlignment)2;
		((Control)ADDoperator).Anchor = (AnchorStyles)6;
		((Control)ADDoperator).Location = new Point(744, 333);
		((Control)ADDoperator).Name = "ADDoperator";
		((Control)ADDoperator).Size = new Size(165, 33);
		((Control)ADDoperator).TabIndex = 5;
		((ButtonBase)ADDoperator).Text = "Добавить";
		((ButtonBase)ADDoperator).UseVisualStyleBackColor = true;
		((Control)PathB).Anchor = (AnchorStyles)6;
		((Control)PathB).Location = new Point(427, 303);
		((Control)PathB).Name = "PathB";
		((Control)PathB).Size = new Size(80, 24);
		((Control)PathB).TabIndex = 6;
		((ButtonBase)PathB).Text = "Путь";
		((ButtonBase)PathB).UseVisualStyleBackColor = true;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(6f, 13f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(921, 378);
		((Control)this).Controls.Add((Control)(object)PathB);
		((Control)this).Controls.Add((Control)(object)ADDoperator);
		((Control)this).Controls.Add((Control)(object)Oinn);
		((Control)this).Controls.Add((Control)(object)Opass);
		((Control)this).Controls.Add((Control)(object)Opath);
		((Control)this).Controls.Add((Control)(object)Oname);
		((Control)this).Controls.Add((Control)(object)DG);
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Control)this).Name = "Form2";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "OPERATORS";
		((ISupportInitialize)DG).EndInit();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void Form2_Load(object sender, EventArgs e)
	{
		LoadOperators();
	}

	private void LoadOperators()
	{
		checked
		{
			try
			{
				DG.RowCount = 0;
				string connectionString = "Data Source=" + WebCheck.All.FileN + "; Version=3";
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = connectionString;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "Select * FROM OPERATORS";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				while (sQLiteDataReader.Read())
				{
					DataGridView dG;
					(dG = DG).RowCount = dG.RowCount + 1;
					DG[0, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[0]);
					DG[1, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[1]);
					DG[2, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[2]);
					DG[3, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[3]);
					DG[4, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[4]);
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
				((Form)this).Text = "OPERATORS " + WebCheck.All.l.MaxID("OPERATORS");
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ProjectData.ClearProjectError();
			}
		}
	}

	private void ADDoperator_Click(object sender, EventArgs e)
	{
		if (Operators.CompareString(Oname.Text.Trim(), "", false) == 0)
		{
			((Control)Oname).Focus();
			return;
		}
		if (Operators.CompareString(Opath.Text.Trim(), "", false) == 0)
		{
			((Control)Opath).Focus();
			return;
		}
		if (Operators.CompareString(Opass.Text.Trim(), "", false) == 0)
		{
			((Control)Opass).Focus();
			return;
		}
		if ((Operators.CompareString(Oinn.Text.Trim(), "", false) == 0) | (Oinn.Text.Length < 10))
		{
			((Control)Oinn).Focus();
			return;
		}
		if (!Versioned.IsNumeric((object)Oinn.Text))
		{
			Oinn.Text = "";
			((Control)Oinn).Focus();
			return;
		}
		string connectionString = "Data Source=" + WebCheck.All.FileN + "; Version=3";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = connectionString;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "INSERT INTO OPERATORS (OPERATORNAME, KEYPATH, KEYPASS, INN ) VALUES ('" + Oname.Text + "','" + Opath.Text + "','" + Opass.Text + "','" + Oinn.Text + "')";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
		Oname.Text = "";
		Opath.Text = "";
		Opass.Text = "";
		Oinn.Text = "";
		LoadOperators();
	}

	private void PathB_Click(object sender, EventArgs e)
	{
		//IL_0000: Unknown result type (might be due to invalid IL or missing references)
		//IL_0006: Expected O, but got Unknown
		//IL_0012: Unknown result type (might be due to invalid IL or missing references)
		//IL_0018: Invalid comparison between Unknown and I4
		OpenFileDialog val = new OpenFileDialog();
		((FileDialog)val).Filter = "All Files|*.*";
		if ((int)((CommonDialog)val).ShowDialog() == 1)
		{
			Opath.Text = ((FileDialog)val).FileName;
		}
	}

	private void Oinn_TextChanged(object sender, EventArgs e)
	{
		if (Oinn.Text.Length > 10)
		{
			Oinn.Text = Strings.Mid(Oinn.Text, 1, 10);
		}
	}
}
