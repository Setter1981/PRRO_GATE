using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Gma.QrCodeNet.Encoding;
using Gma.QrCodeNet.Encoding.Windows.Forms;
using Gma.QrCodeNet.Encoding.Windows.Render;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class FormReports : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("DG")]
	private DataGridView _DG;

	[CompilerGenerated]
	[AccessedThroughProperty("QrCode")]
	private QrCodeImgControl _QrCode;

	[CompilerGenerated]
	[AccessedThroughProperty("DG1")]
	private DataGridView _DG1;

	[CompilerGenerated]
	[AccessedThroughProperty("RetSheft")]
	private Button _RetSheft;

	[CompilerGenerated]
	[AccessedThroughProperty("Druk")]
	private Button _Druk;

	[CompilerGenerated]
	[AccessedThroughProperty("Button1")]
	private Button _Button1;

	[CompilerGenerated]
	[AccessedThroughProperty("Zakrit")]
	private Button _Zakrit;

	[CompilerGenerated]
	[AccessedThroughProperty("Acb")]
	private CheckBox _Acb;

	[CompilerGenerated]
	[AccessedThroughProperty("Mcb")]
	private ComboBox _Mcb;

	[CompilerGenerated]
	[AccessedThroughProperty("Ycb")]
	private ComboBox _Ycb;

	[CompilerGenerated]
	[AccessedThroughProperty("VibB")]
	private Button _VibB;

	private bool VidShift;

	private int LocalNumerCheck;

	private Image PrintLogo;

	internal virtual DataGridView DG
	{
		[CompilerGenerated]
		get
		{
			return _DG;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			DataGridViewCellEventHandler value2 = DG_CellDoubleClick;
			DataGridView dG = _DG;
			if (dG != null)
			{
				dG.CellDoubleClick -= value2;
			}
			_DG = value;
			dG = _DG;
			if (dG != null)
			{
				dG.CellDoubleClick += value2;
			}
		}
	}

	[field: AccessedThroughProperty("Tb")]
	internal virtual TextBox Tb
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual QrCodeImgControl QrCode
	{
		[CompilerGenerated]
		get
		{
			return _QrCode;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = QrCode_DoubleClick;
			EventHandler value3 = QrCode_Click;
			QrCodeImgControl qrCode = _QrCode;
			if (qrCode != null)
			{
				qrCode.DoubleClick -= value2;
				qrCode.Click -= value3;
			}
			_QrCode = value;
			qrCode = _QrCode;
			if (qrCode != null)
			{
				qrCode.DoubleClick += value2;
				qrCode.Click += value3;
			}
		}
	}

	internal virtual DataGridView DG1
	{
		[CompilerGenerated]
		get
		{
			return _DG1;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			DataGridViewCellEventHandler value2 = DG1_CellDoubleClick;
			DataGridView dG = _DG1;
			if (dG != null)
			{
				dG.CellDoubleClick -= value2;
			}
			_DG1 = value;
			dG = _DG1;
			if (dG != null)
			{
				dG.CellDoubleClick += value2;
			}
		}
	}

	internal virtual Button RetSheft
	{
		[CompilerGenerated]
		get
		{
			return _RetSheft;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = RetSheft_Click;
			Button retSheft = _RetSheft;
			if (retSheft != null)
			{
				retSheft.Click -= value2;
			}
			_RetSheft = value;
			retSheft = _RetSheft;
			if (retSheft != null)
			{
				retSheft.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("GB")]
	internal virtual GroupBox GB
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("GB1")]
	internal virtual GroupBox GB1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TB2")]
	internal virtual TextBox TB2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("T1")]
	internal virtual TextBox T1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("T2")]
	internal virtual TextBox T2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("T3")]
	internal virtual TextBox T3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("T4")]
	internal virtual TextBox T4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button Druk
	{
		[CompilerGenerated]
		get
		{
			return _Druk;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = Druk_Click;
			Button druk = _Druk;
			if (druk != null)
			{
				druk.Click -= value2;
			}
			_Druk = value;
			druk = _Druk;
			if (druk != null)
			{
				druk.Click += value2;
			}
		}
	}

	internal virtual Button Button1
	{
		[CompilerGenerated]
		get
		{
			return _Button1;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = Button1_Click;
			Button button = _Button1;
			if (button != null)
			{
				button.Click -= value2;
			}
			_Button1 = value;
			button = _Button1;
			if (button != null)
			{
				button.Click += value2;
			}
		}
	}

	internal virtual Button Zakrit
	{
		[CompilerGenerated]
		get
		{
			return _Zakrit;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = Zakrit_Click;
			Button zakrit = _Zakrit;
			if (zakrit != null)
			{
				zakrit.Click -= value2;
			}
			_Zakrit = value;
			zakrit = _Zakrit;
			if (zakrit != null)
			{
				zakrit.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("PrintDialog1")]
	internal virtual PrintDialog PrintDialog1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("DataGridViewTextBoxColumn1")]
	internal virtual DataGridViewTextBoxColumn DataGridViewTextBoxColumn1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("DataGridViewTextBoxColumn2")]
	internal virtual DataGridViewTextBoxColumn DataGridViewTextBoxColumn2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("DataGridViewTextBoxColumn3")]
	internal virtual DataGridViewTextBoxColumn DataGridViewTextBoxColumn3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("DataGridViewTextBoxColumn4")]
	internal virtual DataGridViewTextBoxColumn DataGridViewTextBoxColumn4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("DataGridViewTextBoxColumn5")]
	internal virtual DataGridViewTextBoxColumn DataGridViewTextBoxColumn5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column6")]
	internal virtual DataGridViewTextBoxColumn Column6
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column1")]
	internal virtual DataGridViewTextBoxColumn Column1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column2")]
	internal virtual DataGridViewTextBoxColumn Column2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column3")]
	internal virtual DataGridViewTextBoxColumn Column3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column4")]
	internal virtual DataGridViewTextBoxColumn Column4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column5")]
	internal virtual DataGridViewTextBoxColumn Column5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual CheckBox Acb
	{
		[CompilerGenerated]
		get
		{
			return _Acb;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = Acb_CheckedChanged;
			CheckBox acb = _Acb;
			if (acb != null)
			{
				acb.CheckedChanged -= value2;
			}
			_Acb = value;
			acb = _Acb;
			if (acb != null)
			{
				acb.CheckedChanged += value2;
			}
		}
	}

	internal virtual ComboBox Mcb
	{
		[CompilerGenerated]
		get
		{
			return _Mcb;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = Mcb_SelectedIndexChanged;
			ComboBox mcb = _Mcb;
			if (mcb != null)
			{
				mcb.SelectedIndexChanged -= value2;
			}
			_Mcb = value;
			mcb = _Mcb;
			if (mcb != null)
			{
				mcb.SelectedIndexChanged += value2;
			}
		}
	}

	internal virtual ComboBox Ycb
	{
		[CompilerGenerated]
		get
		{
			return _Ycb;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = Ycb_SelectedIndexChanged;
			ComboBox ycb = _Ycb;
			if (ycb != null)
			{
				ycb.SelectedIndexChanged -= value2;
			}
			_Ycb = value;
			ycb = _Ycb;
			if (ycb != null)
			{
				ycb.SelectedIndexChanged += value2;
			}
		}
	}

	[field: AccessedThroughProperty("Label4")]
	internal virtual Label Label4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label3")]
	internal virtual Label Label3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label2")]
	internal virtual Label Label2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label5")]
	internal virtual Label Label5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button VibB
	{
		[CompilerGenerated]
		get
		{
			return _VibB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = VibB_Click;
			Button vibB = _VibB;
			if (vibB != null)
			{
				vibB.Click -= value2;
			}
			_VibB = value;
			vibB = _VibB;
			if (vibB != null)
			{
				vibB.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("ToolTip1")]
	internal virtual ToolTip ToolTip1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public FormReports()
	{
		base.Load += FormReports_Load;
		VidShift = true;
		LocalNumerCheck = 0;
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
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		this.components = new System.ComponentModel.Container();
		System.Windows.Forms.DataGridViewCellStyle dataGridViewCellStyle = new System.Windows.Forms.DataGridViewCellStyle();
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormReports));
		System.Windows.Forms.DataGridViewCellStyle dataGridViewCellStyle2 = new System.Windows.Forms.DataGridViewCellStyle();
		this.DG = new System.Windows.Forms.DataGridView();
		this.Column1 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column2 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column3 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column4 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column5 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Tb = new System.Windows.Forms.TextBox();
		this.QrCode = new Gma.QrCodeNet.Encoding.Windows.Forms.QrCodeImgControl();
		this.DG1 = new System.Windows.Forms.DataGridView();
		this.DataGridViewTextBoxColumn1 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.DataGridViewTextBoxColumn2 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.DataGridViewTextBoxColumn3 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.DataGridViewTextBoxColumn4 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.DataGridViewTextBoxColumn5 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column6 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.RetSheft = new System.Windows.Forms.Button();
		this.GB = new System.Windows.Forms.GroupBox();
		this.Label5 = new System.Windows.Forms.Label();
		this.Label4 = new System.Windows.Forms.Label();
		this.Label3 = new System.Windows.Forms.Label();
		this.Label2 = new System.Windows.Forms.Label();
		this.Label1 = new System.Windows.Forms.Label();
		this.Acb = new System.Windows.Forms.CheckBox();
		this.T4 = new System.Windows.Forms.TextBox();
		this.Ycb = new System.Windows.Forms.ComboBox();
		this.T2 = new System.Windows.Forms.TextBox();
		this.T3 = new System.Windows.Forms.TextBox();
		this.Mcb = new System.Windows.Forms.ComboBox();
		this.T1 = new System.Windows.Forms.TextBox();
		this.GB1 = new System.Windows.Forms.GroupBox();
		this.VibB = new System.Windows.Forms.Button();
		this.Zakrit = new System.Windows.Forms.Button();
		this.Button1 = new System.Windows.Forms.Button();
		this.Druk = new System.Windows.Forms.Button();
		this.TB2 = new System.Windows.Forms.TextBox();
		this.PrintDialog1 = new System.Windows.Forms.PrintDialog();
		this.ToolTip1 = new System.Windows.Forms.ToolTip(this.components);
		((System.ComponentModel.ISupportInitialize)this.DG).BeginInit();
		((System.ComponentModel.ISupportInitialize)this.QrCode).BeginInit();
		((System.ComponentModel.ISupportInitialize)this.DG1).BeginInit();
		this.GB.SuspendLayout();
		this.GB1.SuspendLayout();
		base.SuspendLayout();
		this.DG.AllowUserToAddRows = false;
		this.DG.AllowUserToDeleteRows = false;
		this.DG.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Bottom | System.Windows.Forms.AnchorStyles.Left | System.Windows.Forms.AnchorStyles.Right;
		this.DG.ColumnHeadersHeightSizeMode = System.Windows.Forms.DataGridViewColumnHeadersHeightSizeMode.AutoSize;
		this.DG.Columns.AddRange(this.Column1, this.Column2, this.Column3, this.Column4, this.Column5);
		this.DG.Location = new System.Drawing.Point(624, 12);
		this.DG.MultiSelect = false;
		this.DG.Name = "DG";
		this.DG.ReadOnly = true;
		this.DG.RowHeadersWidth = 51;
		this.DG.RowTemplate.Height = 24;
		this.DG.Size = new System.Drawing.Size(701, 587);
		this.DG.TabIndex = 0;
		dataGridViewCellStyle.NullValue = null;
		this.Column1.DefaultCellStyle = dataGridViewCellStyle;
		this.Column1.HeaderText = "№";
		this.Column1.MinimumWidth = 6;
		this.Column1.Name = "Column1";
		this.Column1.ReadOnly = true;
		this.Column1.Width = 50;
		this.Column2.HeaderText = "Відкрита";
		this.Column2.MinimumWidth = 6;
		this.Column2.Name = "Column2";
		this.Column2.ReadOnly = true;
		this.Column2.Width = 125;
		this.Column3.HeaderText = "Закрита";
		this.Column3.MinimumWidth = 6;
		this.Column3.Name = "Column3";
		this.Column3.ReadOnly = true;
		this.Column3.Width = 125;
		this.Column4.HeaderText = "Оператор";
		this.Column4.MinimumWidth = 6;
		this.Column4.Name = "Column4";
		this.Column4.ReadOnly = true;
		this.Column4.Width = 125;
		this.Column5.HeaderText = "Чеків";
		this.Column5.MinimumWidth = 6;
		this.Column5.Name = "Column5";
		this.Column5.ReadOnly = true;
		this.Column5.SortMode = System.Windows.Forms.DataGridViewColumnSortMode.NotSortable;
		this.Column5.Width = 50;
		this.Tb.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Bottom | System.Windows.Forms.AnchorStyles.Left;
		this.Tb.BackColor = System.Drawing.Color.White;
		this.Tb.BorderStyle = System.Windows.Forms.BorderStyle.None;
		this.Tb.Font = new System.Drawing.Font("Consolas", 9f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Tb.Location = new System.Drawing.Point(12, 245);
		this.Tb.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.Tb.Multiline = true;
		this.Tb.Name = "Tb";
		this.Tb.ReadOnly = true;
		this.Tb.ScrollBars = System.Windows.Forms.ScrollBars.Vertical;
		this.Tb.Size = new System.Drawing.Size(606, 354);
		this.Tb.TabIndex = 2;
		this.Tb.TabStop = false;
		this.Tb.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.QrCode.ErrorCorrectLevel = Gma.QrCodeNet.Encoding.ErrorCorrectionLevel.M;
		this.QrCode.Image = (System.Drawing.Image)resources.GetObject("QrCode.Image");
		this.QrCode.Location = new System.Drawing.Point(11, 28);
		this.QrCode.Name = "QrCode";
		this.QrCode.QuietZoneModule = Gma.QrCodeNet.Encoding.Windows.Render.QuietZoneModules.Two;
		this.QrCode.Size = new System.Drawing.Size(198, 198);
		this.QrCode.SizeMode = System.Windows.Forms.PictureBoxSizeMode.Zoom;
		this.QrCode.TabIndex = 6;
		this.QrCode.TabStop = false;
		this.QrCode.Text = "WebCheck";
		this.ToolTip1.SetToolTip(this.QrCode, "QR код чека");
		this.DG1.AllowUserToAddRows = false;
		this.DG1.AllowUserToDeleteRows = false;
		this.DG1.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Bottom | System.Windows.Forms.AnchorStyles.Left | System.Windows.Forms.AnchorStyles.Right;
		this.DG1.ColumnHeadersHeightSizeMode = System.Windows.Forms.DataGridViewColumnHeadersHeightSizeMode.AutoSize;
		this.DG1.Columns.AddRange(this.DataGridViewTextBoxColumn1, this.DataGridViewTextBoxColumn2, this.DataGridViewTextBoxColumn3, this.DataGridViewTextBoxColumn4, this.DataGridViewTextBoxColumn5, this.Column6);
		this.DG1.Location = new System.Drawing.Point(652, 56);
		this.DG1.MultiSelect = false;
		this.DG1.Name = "DG1";
		this.DG1.ReadOnly = true;
		this.DG1.RowHeadersWidth = 51;
		this.DG1.RowTemplate.Height = 24;
		this.DG1.Size = new System.Drawing.Size(701, 587);
		this.DG1.TabIndex = 7;
		dataGridViewCellStyle2.NullValue = null;
		this.DataGridViewTextBoxColumn1.DefaultCellStyle = dataGridViewCellStyle2;
		this.DataGridViewTextBoxColumn1.HeaderText = "№";
		this.DataGridViewTextBoxColumn1.MinimumWidth = 6;
		this.DataGridViewTextBoxColumn1.Name = "DataGridViewTextBoxColumn1";
		this.DataGridViewTextBoxColumn1.ReadOnly = true;
		this.DataGridViewTextBoxColumn1.Width = 50;
		this.DataGridViewTextBoxColumn2.HeaderText = "Ідентифікатор";
		this.DataGridViewTextBoxColumn2.MinimumWidth = 6;
		this.DataGridViewTextBoxColumn2.Name = "DataGridViewTextBoxColumn2";
		this.DataGridViewTextBoxColumn2.ReadOnly = true;
		this.DataGridViewTextBoxColumn2.SortMode = System.Windows.Forms.DataGridViewColumnSortMode.NotSortable;
		this.DataGridViewTextBoxColumn2.Width = 108;
		this.DataGridViewTextBoxColumn3.HeaderText = "Тип";
		this.DataGridViewTextBoxColumn3.MinimumWidth = 6;
		this.DataGridViewTextBoxColumn3.Name = "DataGridViewTextBoxColumn3";
		this.DataGridViewTextBoxColumn3.ReadOnly = true;
		this.DataGridViewTextBoxColumn3.Width = 75;
		this.DataGridViewTextBoxColumn4.HeaderText = "Дата";
		this.DataGridViewTextBoxColumn4.MinimumWidth = 6;
		this.DataGridViewTextBoxColumn4.Name = "DataGridViewTextBoxColumn4";
		this.DataGridViewTextBoxColumn4.ReadOnly = true;
		this.DataGridViewTextBoxColumn4.Width = 127;
		this.DataGridViewTextBoxColumn5.HeaderText = "Сума";
		this.DataGridViewTextBoxColumn5.MinimumWidth = 6;
		this.DataGridViewTextBoxColumn5.Name = "DataGridViewTextBoxColumn5";
		this.DataGridViewTextBoxColumn5.ReadOnly = true;
		this.DataGridViewTextBoxColumn5.Width = 125;
		this.Column6.HeaderText = "ID";
		this.Column6.MinimumWidth = 6;
		this.Column6.Name = "Column6";
		this.Column6.ReadOnly = true;
		this.Column6.Visible = false;
		this.Column6.Width = 125;
		this.RetSheft.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Bold, System.Drawing.GraphicsUnit.Point, 204);
		this.RetSheft.Location = new System.Drawing.Point(558, 12);
		this.RetSheft.Name = "RetSheft";
		this.RetSheft.Size = new System.Drawing.Size(60, 40);
		this.RetSheft.TabIndex = 8;
		this.RetSheft.Text = "<<";
		this.ToolTip1.SetToolTip(this.RetSheft, "Повернися до списку змін ");
		this.RetSheft.UseVisualStyleBackColor = true;
		this.GB.Controls.Add(this.Label5);
		this.GB.Controls.Add(this.Label4);
		this.GB.Controls.Add(this.Label3);
		this.GB.Controls.Add(this.Label2);
		this.GB.Controls.Add(this.Label1);
		this.GB.Controls.Add(this.Acb);
		this.GB.Controls.Add(this.T4);
		this.GB.Controls.Add(this.Ycb);
		this.GB.Controls.Add(this.T2);
		this.GB.Controls.Add(this.T3);
		this.GB.Controls.Add(this.Mcb);
		this.GB.Controls.Add(this.T1);
		this.GB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.GB.Location = new System.Drawing.Point(12, 4);
		this.GB.Name = "GB";
		this.GB.Size = new System.Drawing.Size(540, 236);
		this.GB.TabIndex = 9;
		this.GB.TabStop = false;
		this.GB.Text = "ФН";
		this.Label5.AutoSize = true;
		this.Label5.Location = new System.Drawing.Point(6, 192);
		this.Label5.Name = "Label5";
		this.Label5.Size = new System.Drawing.Size(101, 25);
		this.Label5.TabIndex = 19;
		this.Label5.Text = "Показати";
		this.Label4.AutoSize = true;
		this.Label4.Location = new System.Drawing.Point(28, 131);
		this.Label4.Name = "Label4";
		this.Label4.Size = new System.Drawing.Size(79, 25);
		this.Label4.TabIndex = 18;
		this.Label4.Text = "Адреса";
		this.Label3.AutoSize = true;
		this.Label3.Location = new System.Drawing.Point(41, 75);
		this.Label3.Name = "Label3";
		this.Label3.Size = new System.Drawing.Size(66, 25);
		this.Label3.TabIndex = 17;
		this.Label3.Text = "Назва";
		this.Label2.AutoSize = true;
		this.Label2.Location = new System.Drawing.Point(303, 35);
		this.Label2.Name = "Label2";
		this.Label2.Size = new System.Drawing.Size(44, 25);
		this.Label2.TabIndex = 16;
		this.Label2.Text = "TIN";
		this.Label1.AutoSize = true;
		this.Label1.Location = new System.Drawing.Point(62, 35);
		this.Label1.Name = "Label1";
		this.Label1.Size = new System.Drawing.Size(45, 25);
		this.Label1.TabIndex = 15;
		this.Label1.Text = "INN";
		this.Acb.AutoSize = true;
		this.Acb.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Acb.Location = new System.Drawing.Point(463, 191);
		this.Acb.Name = "Acb";
		this.Acb.Size = new System.Drawing.Size(60, 29);
		this.Acb.TabIndex = 12;
		this.Acb.Text = "Всі";
		this.Acb.UseVisualStyleBackColor = true;
		this.T4.Enabled = false;
		this.T4.Font = new System.Drawing.Font("Microsoft Sans Serif", 13.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.T4.Location = new System.Drawing.Point(125, 108);
		this.T4.Multiline = true;
		this.T4.Name = "T4";
		this.T4.Size = new System.Drawing.Size(398, 63);
		this.T4.TabIndex = 11;
		this.T4.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Ycb.DropDownStyle = System.Windows.Forms.ComboBoxStyle.DropDownList;
		this.Ycb.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Ycb.FormattingEnabled = true;
		this.Ycb.Location = new System.Drawing.Point(344, 193);
		this.Ycb.Name = "Ycb";
		this.Ycb.Size = new System.Drawing.Size(99, 28);
		this.Ycb.TabIndex = 14;
		this.T2.Enabled = false;
		this.T2.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.T2.Location = new System.Drawing.Point(356, 32);
		this.T2.Name = "T2";
		this.T2.Size = new System.Drawing.Size(167, 30);
		this.T2.TabIndex = 10;
		this.T2.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.T3.Enabled = false;
		this.T3.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.T3.Location = new System.Drawing.Point(125, 72);
		this.T3.Name = "T3";
		this.T3.Size = new System.Drawing.Size(398, 30);
		this.T3.TabIndex = 9;
		this.T3.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Mcb.DropDownStyle = System.Windows.Forms.ComboBoxStyle.DropDownList;
		this.Mcb.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Mcb.FormattingEnabled = true;
		this.Mcb.Location = new System.Drawing.Point(125, 191);
		this.Mcb.Name = "Mcb";
		this.Mcb.Size = new System.Drawing.Size(163, 28);
		this.Mcb.TabIndex = 13;
		this.T1.Enabled = false;
		this.T1.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.T1.Location = new System.Drawing.Point(125, 32);
		this.T1.Name = "T1";
		this.T1.Size = new System.Drawing.Size(167, 30);
		this.T1.TabIndex = 8;
		this.T1.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.GB1.Controls.Add(this.VibB);
		this.GB1.Controls.Add(this.Zakrit);
		this.GB1.Controls.Add(this.Button1);
		this.GB1.Controls.Add(this.Druk);
		this.GB1.Controls.Add(this.TB2);
		this.GB1.Controls.Add(this.QrCode);
		this.GB1.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.GB1.Location = new System.Drawing.Point(12, 267);
		this.GB1.Name = "GB1";
		this.GB1.Size = new System.Drawing.Size(540, 236);
		this.GB1.TabIndex = 10;
		this.GB1.TabStop = false;
		this.GB1.Text = "Чек";
		this.VibB.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right;
		this.VibB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.VibB.Location = new System.Drawing.Point(222, 109);
		this.VibB.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.VibB.Name = "VibB";
		this.VibB.Size = new System.Drawing.Size(308, 46);
		this.VibB.TabIndex = 11;
		this.VibB.Text = "VIBER/SMS...";
		this.ToolTip1.SetToolTip(this.VibB, "Надіслати чек покупцю на Вайбер або СМС ");
		this.VibB.UseVisualStyleBackColor = true;
		this.Zakrit.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right;
		this.Zakrit.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Zakrit.Location = new System.Drawing.Point(222, 171);
		this.Zakrit.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.Zakrit.Name = "Zakrit";
		this.Zakrit.Size = new System.Drawing.Size(151, 46);
		this.Zakrit.TabIndex = 10;
		this.Zakrit.Text = "Принтер...";
		this.ToolTip1.SetToolTip(this.Zakrit, "Вибір принтера для друку ");
		this.Zakrit.UseVisualStyleBackColor = true;
		this.Button1.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right;
		this.Button1.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Button1.Location = new System.Drawing.Point(494, 29);
		this.Button1.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.Button1.Name = "Button1";
		this.Button1.Size = new System.Drawing.Size(36, 34);
		this.Button1.TabIndex = 9;
		this.Button1.Text = "C";
		this.ToolTip1.SetToolTip(this.Button1, "Копіювати лінк на чек у буфер обміну");
		this.Button1.UseVisualStyleBackColor = true;
		this.Druk.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right;
		this.Druk.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Druk.Location = new System.Drawing.Point(379, 171);
		this.Druk.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.Druk.Name = "Druk";
		this.Druk.Size = new System.Drawing.Size(151, 46);
		this.Druk.TabIndex = 8;
		this.Druk.Text = "ДРУК";
		this.ToolTip1.SetToolTip(this.Druk, "Друк вибраного чека ");
		this.Druk.UseVisualStyleBackColor = true;
		this.TB2.Enabled = false;
		this.TB2.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TB2.Location = new System.Drawing.Point(222, 29);
		this.TB2.Name = "TB2";
		this.TB2.Size = new System.Drawing.Size(266, 30);
		this.TB2.TabIndex = 7;
		this.TB2.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.PrintDialog1.UseEXDialog = true;
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(1337, 611);
		base.Controls.Add(this.GB1);
		base.Controls.Add(this.GB);
		base.Controls.Add(this.RetSheft);
		base.Controls.Add(this.DG1);
		base.Controls.Add(this.Tb);
		base.Controls.Add(this.DG);
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormReports";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Просмотр чеков";
		((System.ComponentModel.ISupportInitialize)this.DG).EndInit();
		((System.ComponentModel.ISupportInitialize)this.QrCode).EndInit();
		((System.ComponentModel.ISupportInitialize)this.DG1).EndInit();
		this.GB.ResumeLayout(false);
		this.GB.PerformLayout();
		this.GB1.ResumeLayout(false);
		this.GB1.PerformLayout();
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	private void LoadImg()
	{
		try
		{
			string text = All.MyDoc() + "\\WebCheck\\Logo\\" + All.A.FN + ".bmp";
			if (File.Exists(text))
			{
				PrintLogo = Image.FromFile(text);
				return;
			}
			text = All.MyDoc() + "\\WebCheck\\Logo\\" + All.A.FN + ".jpg";
			if (File.Exists(text))
			{
				PrintLogo = Image.FromFile(text);
				return;
			}
			text = All.MyDoc() + "\\WebCheck\\Logo\\" + All.A.FN + ".png";
			if (File.Exists(text))
			{
				PrintLogo = Image.FromFile(text);
				return;
			}
			text = All.MyDoc() + "\\WebCheck\\logo.png";
			if (File.Exists(text))
			{
				PrintLogo = Image.FromFile(text);
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	private void FormReports_Load(object sender, EventArgs e)
	{
		Application.DoEvents();
		DG.Visible = true;
		DG1.Visible = false;
		GB.Visible = true;
		GB1.Visible = false;
		VidShift = true;
		RetSheft.Enabled = false;
		DG1.Left = DG.Left;
		DG1.Top = DG.Top;
		GB1.Left = GB.Left;
		GB1.Top = GB.Top;
		VibB.Enabled = false;
		if (Operators.CompareString(All.A.FiscalMode, All.URLfact, TextCompare: false) == 0)
		{
			GB.Text = "ФН " + All.A.FN;
		}
		else
		{
			GB.Text = "ФН " + All.A.FN + "_TS";
		}
		T1.Text = All.A.INN;
		T2.Text = All.A.TIN;
		T3.Text = All.A.PointName;
		T4.Text = All.A.PointAddr;
		LoadImg();
		int year = DateTime.Now.Year;
		int num = year;
		for (int i = 2019; i <= num; i = checked(i + 1))
		{
			Ycb.Items.Add(i.ToString());
		}
		Ycb.Text = Conversions.ToString(year);
		Mcb.Items.Add("Січень");
		Mcb.Items.Add("Лютий");
		Mcb.Items.Add("Березень");
		Mcb.Items.Add("Квітень");
		Mcb.Items.Add("Травень");
		Mcb.Items.Add("Червень");
		Mcb.Items.Add("Липень");
		Mcb.Items.Add("Серпень");
		Mcb.Items.Add("Вересень");
		Mcb.Items.Add("Жовтень");
		Mcb.Items.Add("Листопад");
		Mcb.Items.Add("Грудень");
		Mcb.Text = IntToMoon(DateTime.Now.Month.ToString());
		QrCode.Text = "http://www.webchek.com.ua";
		Shefts(MoonToInt(Mcb.Text), Ycb.Text);
		Application.DoEvents();
	}

	private string MoonToInt(string Ms)
	{
		return Ms switch
		{
			"Січень" => "01", 
			"Лютий" => "02", 
			"Березень" => "03", 
			"Квітень" => "04", 
			"Травень" => "05", 
			"Червень" => "06", 
			"Липень" => "07", 
			"Серпень" => "08", 
			"Вересень" => "09", 
			"Жовтень" => "10", 
			"Листопад" => "11", 
			"Грудень" => "12", 
			_ => "", 
		};
	}

	private string IntToMoon(string Mi)
	{
		return Mi switch
		{
			"1" => "Січень", 
			"2" => "Лютий", 
			"3" => "Березень", 
			"4" => "Квітень", 
			"5" => "Травень", 
			"6" => "Червень", 
			"7" => "Липень", 
			"8" => "Серпень", 
			"9" => "Вересень", 
			"10" => "Жовтень", 
			"11" => "Листопад", 
			"12" => "Грудень", 
			_ => "", 
		};
	}

	private bool ShiftsVisible(int nCheck)
	{
		ShiftsVisible();
		AllDoc(nCheck);
		return VidShift;
	}

	private bool ShiftsVisible()
	{
		VibB.Enabled = false;
		GB1.Enabled = false;
		if (DG.Visible)
		{
			DG.Visible = false;
			DG1.Visible = true;
			GB.Visible = false;
			GB1.Visible = true;
			VidShift = false;
			RetSheft.Enabled = true;
			return VidShift;
		}
		DG.Visible = true;
		DG1.Visible = false;
		GB.Visible = true;
		GB1.Visible = false;
		VidShift = true;
		RetSheft.Enabled = false;
		DG1.RowCount = 0;
		QrCode.Text = "http://www.webchek.com.ua";
		Tb.Text = "";
		TB2.Text = "";
		return VidShift;
	}

	private void Shefts()
	{
		checked
		{
			try
			{
				ShiftsAll shiftsAll = new ShiftsAll();
				DG.RowCount = 0;
				int shifts = shiftsAll.Shifts;
				for (int i = 1; i <= shifts; i++)
				{
					DG.RowCount++;
					DG[0, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(0, i);
					DG[1, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(1, i);
					DG[2, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(2, i);
					DG[3, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(3, i);
					DG[4, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(4, i);
				}
				Text = "Зміни";
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ProjectData.ClearProjectError();
			}
		}
	}

	private void Shefts(string Ms, string Ys)
	{
		checked
		{
			try
			{
				ShiftsAll shiftsAll = new ShiftsAll(Ms.Trim(), Ys.Trim());
				DG.RowCount = 0;
				int shifts = shiftsAll.Shifts;
				for (int i = 1; i <= shifts; i++)
				{
					DG.RowCount++;
					DG[0, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(0, i);
					DG[1, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(1, i);
					DG[2, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(2, i);
					DG[3, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(3, i);
					DG[4, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(4, i);
				}
				Text = "Зміни";
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ProjectData.ClearProjectError();
			}
		}
	}

	private void CheckN(int numCheck)
	{
		checked
		{
			try
			{
				CheckShiftAll checkShiftAll = new CheckShiftAll(numCheck);
				int checks = checkShiftAll.Checks;
				for (int i = 1; i <= checks; i++)
				{
					DG1.RowCount++;
					DG1[0, DG1.RowCount - 1].Value = checkShiftAll.get_InfaCheck(0, i);
					DG1[1, DG1.RowCount - 1].Value = checkShiftAll.get_InfaCheck(1, i);
					DG1[2, DG1.RowCount - 1].Value = checkShiftAll.get_InfaCheck(2, i);
					DG1[3, DG1.RowCount - 1].Value = checkShiftAll.get_InfaCheck(3, i);
					DG1[4, DG1.RowCount - 1].Value = All.Bablo(checkShiftAll.get_InfaCheck(4, i));
				}
				Text = "Зміна №" + numCheck;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ProjectData.ClearProjectError();
			}
		}
	}

	private void AllDoc(int numShifts)
	{
		checked
		{
			try
			{
				AllDokShefts allDokShefts = new AllDokShefts(numShifts);
				int checks = allDokShefts.Checks;
				for (int i = 1; i <= checks; i++)
				{
					DG1.RowCount++;
					DG1[0, DG1.RowCount - 1].Value = allDokShefts.get_InfaCheck(0, i);
					DG1[1, DG1.RowCount - 1].Value = allDokShefts.get_InfaCheck(1, i);
					DG1[2, DG1.RowCount - 1].Value = allDokShefts.get_InfaCheck(2, i);
					DG1[3, DG1.RowCount - 1].Value = allDokShefts.get_InfaCheck(3, i);
					DG1[4, DG1.RowCount - 1].Value = All.Bablo(allDokShefts.get_InfaCheck(4, i));
					DG1[5, DG1.RowCount - 1].Value = allDokShefts.get_InfaCheck(5, i);
				}
				Text = "Зміна №" + numShifts;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ProjectData.ClearProjectError();
			}
		}
	}

	private void RetSheft_Click(object sender, EventArgs e)
	{
		ShiftsVisible();
	}

	private void DG_CellDoubleClick(object sender, DataGridViewCellEventArgs e)
	{
		if (e.RowIndex >= 0)
		{
			string value = DG[0, DG.CurrentRow.Index].Value.ToString();
			ShiftsVisible(Conversions.ToInteger(value));
		}
	}

	private void DG1_CellDoubleClick(object sender, DataGridViewCellEventArgs e)
	{
		if (e.RowIndex >= 0)
		{
			string numCheck = DG1[5, DG1.CurrentRow.Index].Value.ToString();
			GB1.Text = "Чек № " + DG1[0, DG1.CurrentRow.Index].Value.ToString();
			CheckZapoln(numCheck);
			if ((Operators.CompareString(DG1[2, DG1.CurrentRow.Index].Value.ToString(), "продаж", TextCompare: false) == 0) | (Operators.CompareString(DG1[2, DG1.CurrentRow.Index].Value.ToString(), "повернення", TextCompare: false) == 0))
			{
				VibB.Enabled = true;
			}
			else
			{
				VibB.Enabled = false;
			}
			GB1.Enabled = true;
		}
	}

	private bool CheckZapoln(string NumCheck)
	{
		PrintExportCheck printExportCheck = new PrintExportCheck();
		switch (All.f.IntegerGetFn(All.A.FN, "PrinterWidth"))
		{
		case 57:
			printExportCheck.Dlstr = 29;
			break;
		case 80:
			printExportCheck.Dlstr = 40;
			break;
		default:
			printExportCheck.Dlstr = 29;
			break;
		}
		QrCode.Text = printExportCheck.CheckVis(NumCheck);
		Tb.Text = printExportCheck.Tb;
		TB2.Text = printExportCheck.Tb2;
		LocalNumerCheck = Conversions.ToInteger(NumCheck);
		return true;
	}

	private void QrCode_DoubleClick(object sender, EventArgs e)
	{
		OpenURL(QrCode.Text);
	}

	public void OpenURL(string wwwURL)
	{
		try
		{
			Process.Start(wwwURL);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	private void Druk_Click(object sender, EventArgs e)
	{
		new PrintExportCheck().PrintCheck(Conversions.ToString(LocalNumerCheck), QrCode.Image, PrintLogo);
	}

	private void Button1_Click(object sender, EventArgs e)
	{
		Clipboard.SetText(QrCode.Text);
	}

	private void Zakrit_Click(object sender, EventArgs e)
	{
		if (PrintDialog1.ShowDialog() == DialogResult.OK)
		{
			All.A.PrinterName = PrintDialog1.PrinterSettings.PrinterName;
			All.f.StringWriteFN(All.A.FN, "PrinterName", All.A.PrinterName);
			new PrintExportCheck().PrintCheck(Conversions.ToString(LocalNumerCheck), QrCode.Image, PrintLogo);
		}
	}

	private void Mcb_SelectedIndexChanged(object sender, EventArgs e)
	{
		Shefts(MoonToInt(Mcb.Text), Ycb.Text);
	}

	private void Ycb_SelectedIndexChanged(object sender, EventArgs e)
	{
		Shefts(MoonToInt(Mcb.Text), Ycb.Text);
	}

	private void Acb_CheckedChanged(object sender, EventArgs e)
	{
		if (Acb.Checked)
		{
			Ycb.Enabled = false;
			Mcb.Enabled = false;
			Shefts();
		}
		else
		{
			Ycb.Enabled = true;
			Mcb.Enabled = true;
			Shefts(MoonToInt(Mcb.Text), Ycb.Text);
		}
	}

	private void VibB_Click(object sender, EventArgs e)
	{
		if (Operators.CompareString(TB2.Text.Trim(), "", TextCompare: false) != 0)
		{
			FormViber formViber = new FormViber(TB2.Text.Trim());
			formViber.ShowDialog();
			formViber.Dispose();
		}
	}

	private void QrCode_Click(object sender, EventArgs e)
	{
		Clipboard.SetText(QrCode.Text);
	}
}
